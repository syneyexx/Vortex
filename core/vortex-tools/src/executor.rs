//! Uitvoerder (W08.02–W08.05, W08.10–W08.12): valideren → capability → precondities →
//! idempotentie-ledger → cache → uitvoeren (timeout, annuleren, retry) → transactie toepassen →
//! postcondities → events. Plannen met effectanalyse draaien fases parallel.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, RunId, TraceId};

use crate::action::{Action, ActionResult, resolve_refs};
use crate::caps::CapabilityHandle;
use crate::contract::Effect;
use crate::error::{ToolError, ToolErrorKind};
use crate::filter::{ResultFilter, ToolCache, trim_middle};
use crate::ledger::{Ledger, LedgerState};
use crate::registry::ToolRegistry;
use crate::sandbox::Sandbox;
use crate::tool::{ToolCtx, ToolOutput};
use crate::txn::FileTxn;

#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub run: Option<RunId>,
    pub trace: TraceId,
    pub cancel: CancellationToken,
    pub caps: CapabilityHandle,
    /// Extra pogingen bij tijdelijke fouten (alleen idempotente tools).
    pub retries: u32,
    /// Alleen preview: schrijftools zetten klaar, er wordt niets toegepast.
    pub dry_run: bool,
    /// Max tekens tool-uitvoer voor het model.
    pub max_output_chars: usize,
    pub filter: ResultFilter,
    /// Record/replay (W17.01): opnemen of afspelen in plaats van echt uitvoeren.
    pub tape: Option<Arc<crate::tape::Tape>>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            run: None,
            trace: TraceId::new(),
            cancel: CancellationToken::new(),
            caps: CapabilityHandle::default(),
            retries: 2,
            dry_run: false,
            max_output_chars: 12_000,
            filter: ResultFilter::default(),
            tape: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Executor {
    pub registry: Arc<ToolRegistry>,
    ledger: Ledger,
    log: EventLog,
    sandbox: Arc<Sandbox>,
    cache: Arc<ToolCache>,
}

fn summarize(v: &serde_json::Value) -> serde_json::Value {
    // Invoer voor events: lange strings ingekort (geen volledige bestandsinhoud in het log).
    match v {
        serde_json::Value::String(s) if s.chars().count() > 300 => serde_json::Value::String(format!(
            "{}… ({} tekens)",
            s.chars().take(300).collect::<String>(),
            s.chars().count()
        )),
        serde_json::Value::Object(m) => {
            serde_json::Value::Object(m.iter().map(|(k, x)| (k.clone(), summarize(x))).collect())
        }
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().take(20).map(summarize).collect()),
        other => other.clone(),
    }
}

impl Executor {
    pub fn new(registry: Arc<ToolRegistry>, log: EventLog, sandbox: Arc<Sandbox>) -> Self {
        Self {
            registry,
            ledger: Ledger::new(log.clone()),
            log,
            sandbox,
            cache: Arc::new(ToolCache::new(Duration::from_secs(600), 512)),
        }
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    pub fn cache(&self) -> &ToolCache {
        &self.cache
    }
    pub fn sandbox(&self) -> &Arc<Sandbox> {
        &self.sandbox
    }

    fn emit(&self, kind: EventKind, opts: &ExecOptions, payload: serde_json::Value) {
        let mut e = Event::new(kind, opts.trace, payload);
        e.run_id = opts.run;
        let _ = self.log.append(e);
    }

    /// Eén toolaanroep (zoals een model hem doet). Actie-ID = `call_id` (idempotentiesleutel).
    pub async fn call(&self, call_id: &str, tool: &str, input: serde_json::Value, opts: &ExecOptions) -> ActionResult {
        self.execute(Action::new(call_id, tool, input), &HashMap::new(), opts).await
    }

    /// Plan uitvoeren: fases uit de effectanalyse, binnen een fase parallel.
    pub async fn execute_plan(&self, actions: Vec<Action>, opts: &ExecOptions) -> Vec<ActionResult> {
        let reg = self.registry.clone();
        let stages = crate::plan::stages(&actions, |n| reg.contract(n));
        let mut prior: HashMap<String, ActionResult> = HashMap::new();
        for stage in stages {
            let futs = stage.iter().map(|&i| {
                let a = actions[i].clone();
                let failed_dep =
                    a.depends_on.iter().chain(a.refs().iter()).find(|d| prior.get(*d).is_some_and(|r| !r.ok)).cloned();
                let prior = &prior;
                async move {
                    match failed_dep {
                        Some(d) => self.fail(
                            &a,
                            Effect::Read,
                            ToolError::new(ToolErrorKind::Precondition, format!("afhankelijkheid {d} faalde")),
                            0,
                            opts,
                        ),
                        None => self.execute(a, prior, opts).await,
                    }
                }
            });
            let results = futures::future::join_all(futs).await;
            for r in results {
                prior.insert(r.action_id.clone(), r);
            }
        }
        actions.iter().filter_map(|a| prior.remove(&a.id)).collect()
    }

    fn fail(&self, a: &Action, effect: Effect, e: ToolError, attempts: u32, opts: &ExecOptions) -> ActionResult {
        let kind = if e.kind == ToolErrorKind::InvalidInput || e.kind == ToolErrorKind::NotFound {
            EventKind::ToolCallInvalid
        } else {
            EventKind::ToolCallFailed
        };
        self.emit(kind, opts, serde_json::json!({"action_id": a.id, "tool": a.tool, "error": e}));
        ActionResult {
            action_id: a.id.clone(),
            tool: a.tool.clone(),
            ok: false,
            output: None,
            error: Some(e),
            effect,
            attempts,
            cached: false,
            replayed: false,
            duration_ms: 0,
        }
    }

    pub async fn execute(&self, a: Action, prior: &HashMap<String, ActionResult>, opts: &ExecOptions) -> ActionResult {
        let Some(tape) = opts.tape.clone() else { return self.execute_live(a, prior, opts).await };
        let input = resolve_refs(&a.input, prior).unwrap_or_else(|_| a.input.clone());
        let input = match self.registry.get(&a.tool) {
            Some(t) => crate::schema::coerce(&t.contract().input_schema, input),
            None => input,
        };
        let key = crate::input_key(&a.tool, &input);
        match tape.mode {
            crate::tape::TapeMode::Record => {
                let r = self.execute_live(a.clone(), prior, opts).await;
                tape.push(&a.tool, &key, &input, &r);
                r
            }
            mode => {
                match tape.lookup(&a.tool, &key) {
                    Some(mut r) => {
                        r.action_id = a.id.clone();
                        self.emit(EventKind::ToolCallCompleted, opts, serde_json::json!({"action_id": a.id, "tool": a.tool, "replayed": true, "tape": true, "ms": 0}));
                        r
                    }
                    None if mode == crate::tape::TapeMode::ReplayOrLive => self.execute_live(a, prior, opts).await,
                    None => {
                        tape.miss(&a.tool, &key);
                        let e = ToolError::new(
                            ToolErrorKind::Precondition,
                            format!("simulatie: geen opname voor {} met deze invoer", a.tool),
                        )
                        .with_hint("neem eerst op (TapeMode::Record) of gebruik ReplayOrLive");
                        self.fail(&a, crate::contract::Effect::Read, e, 0, opts)
                    }
                }
            }
        }
    }

    async fn execute_live(&self, a: Action, prior: &HashMap<String, ActionResult>, opts: &ExecOptions) -> ActionResult {
        let started = Instant::now();
        // 1. Tool + capability.
        let Some(tool) = self.registry.get(&a.tool) else {
            let e = ToolError::new(ToolErrorKind::NotFound, format!("tool '{}' bestaat niet", a.tool))
                .with_hint(format!("bedoelde je: {}", self.registry.closest(&a.tool, 3).join(", ")));
            return self.fail(&a, Effect::Read, e, 0, opts);
        };
        let c = tool.contract().clone();
        if !opts.caps.allows_tool(&c.name) {
            return self.fail(
                &a,
                c.effect,
                ToolError::new(ToolErrorKind::Capability, format!("tool {} niet toegestaan in deze taak", c.name)),
                0,
                opts,
            );
        }
        if let Err(e) = opts.caps.check_effect(c.effect, !c.net_destinations.is_empty()) {
            return self.fail(&a, c.effect, e, 0, opts);
        }
        // 2. Verwijzingen + schema.
        let input = match resolve_refs(&a.input, prior) {
            Ok(v) => crate::schema::coerce(&c.input_schema, v),
            Err(e) => return self.fail(&a, c.effect, ToolError::new(ToolErrorKind::Precondition, e), 0, opts),
        };
        if let Err(errs) = crate::schema::validate(&c.input_schema, &input) {
            let e = ToolError::invalid(errs.join("; ")).with_hint(format!("schema: {}", c.input_schema));
            return self.fail(&a, c.effect, e, 0, opts);
        }
        // 3. Precondities.
        for p in &a.pre {
            if let Some(why) = p.check(&opts.caps, None, None) {
                return self.fail(&a, c.effect, ToolError::new(ToolErrorKind::Precondition, why), 0, opts);
            }
        }
        // 4. Idempotentie (alles met bijwerking of niet-idempotent).
        let ikey = crate::input_key(&c.name, &input);
        let use_ledger = c.effect != Effect::Read || !c.idempotent;
        if use_ledger && !opts.dry_run {
            match self.ledger.begin(&a.id, &c.name, &ikey, c.effect, opts.run.map(|r| r.to_string())) {
                Ok(LedgerState::Done(out)) => {
                    self.emit(
                        EventKind::ToolCallCompleted,
                        opts,
                        serde_json::json!({"action_id": a.id, "tool": c.name, "replayed": true, "ms": 0}),
                    );
                    return ActionResult {
                        action_id: a.id.clone(),
                        tool: c.name.clone(),
                        ok: true,
                        output: Some(out),
                        error: None,
                        effect: c.effect,
                        attempts: 0,
                        cached: false,
                        replayed: true,
                        duration_ms: 0,
                    };
                }
                Ok(LedgerState::Conflict) => {
                    let e = ToolError::invalid(format!("actie-ID {} is al gebruikt met andere invoer", a.id))
                        .with_hint("gebruik een nieuw actie-ID voor een nieuwe actie");
                    return self.fail(&a, c.effect, e, 0, opts);
                }
                Ok(LedgerState::Interrupted) if c.effect == Effect::External => {
                    let e = ToolError::new(
                        ToolErrorKind::Precondition,
                        format!("actie {} is eerder gestart en mogelijk al uitgevoerd (onderbroken)", a.id),
                    )
                    .with_hint("controleer eerst of het effect er al is; gebruik daarna een nieuw actie-ID");
                    return self.fail(&a, c.effect, e, 0, opts);
                }
                Ok(LedgerState::Interrupted) => {
                    // Lokaal + transactioneel: half toegepast kan niet → veilig opnieuw.
                    let _ = self.ledger.retake(&a.id);
                }
                Ok(LedgerState::Fresh | LedgerState::Failed) => {}
                Err(e) => return self.fail(&a, c.effect, ToolError::failed(format!("ledger: {e}")), 0, opts),
            }
        }
        // 5. Cache.
        if c.cacheable
            && c.effect == Effect::Read
            && let Some(out) = self.cache.get(&ikey)
        {
            self.emit(
                EventKind::ToolCallCompleted,
                opts,
                serde_json::json!({"action_id": a.id, "tool": c.name, "cached": true, "ms": 0}),
            );
            return self.finish_ok(&a, &c.name, c.effect, out, 0, true, started, opts);
        }
        self.emit(
            EventKind::ToolCallStarted,
            opts,
            serde_json::json!({"action_id": a.id, "tool": c.name, "input": summarize(&input), "effect": c.effect, "target": a.target, "origin": c.origin}),
        );
        // 6. Uitvoeren met timeout/annuleren/retry; schrijftools in een transactie.
        let txn = (c.effect == Effect::Local).then(|| Arc::new(FileTxn::new()));
        let mut attempts = 0;
        let res = loop {
            attempts += 1;
            let ctx = ToolCtx {
                run: opts.run,
                trace: opts.trace,
                cancel: opts.cancel.child_token(),
                caps: opts.caps.clone(),
                sandbox: self.sandbox.clone(),
                txn: txn.clone(),
            };
            let fut = tool.call(&ctx, input.clone());
            let r = tokio::select! {
                r = tokio::time::timeout(Duration::from_millis(c.timeout_ms), fut) => match r {
                    Ok(r) => r,
                    Err(_) => { ctx.cancel.cancel(); Err(ToolError::new(ToolErrorKind::Timeout, format!("na {} ms", c.timeout_ms))) }
                },
                _ = opts.cancel.cancelled() => { ctx.cancel.cancel(); Err(ToolError::new(ToolErrorKind::Cancelled, "geannuleerd")) }
            };
            match r {
                Err(e) if e.retryable() && c.idempotent && attempts <= opts.retries && !opts.cancel.is_cancelled() => {
                    if let Some(t) = &txn {
                        t.rollback();
                    }
                    tokio::time::sleep(Duration::from_millis(200 * (1 << (attempts - 1)))).await;
                }
                other => break other,
            }
        };
        let mut out = match res {
            Ok(o) => o,
            Err(e) => {
                if let Some(t) = &txn {
                    t.rollback();
                }
                if use_ledger && !opts.dry_run {
                    let _ = self.ledger.finish(&a.id, Err(&e.to_string()));
                }
                return self.fail(&a, c.effect, e, attempts, opts);
            }
        };
        // 7. Postcondities (zien de klaargezette wijzigingen), uitvoerschema, dan toepassen.
        let mut post_err = None;
        if let Some(os) = &c.output_schema
            && let Err(errs) = crate::schema::validate(os, &out.data)
        {
            post_err = Some(format!("uitvoer klopt niet met het contract: {}", errs.join("; ")));
        }
        for p in &a.post {
            if post_err.is_none() {
                post_err = p.check(&opts.caps, Some(&out), txn.as_deref());
            }
        }
        if let Some(t) = &txn
            && !t.is_empty()
        {
            let pv = t.preview();
            if !pv.is_empty() && out.preview.is_none() {
                out.preview = Some(pv);
            }
            if post_err.is_some() || opts.dry_run {
                t.rollback();
            } else if let Err(e) = t.commit() {
                post_err = Some(format!("toepassen mislukt (teruggedraaid): {e}"));
            }
        }
        if let Some(why) = post_err {
            let e = ToolError::new(ToolErrorKind::Postcondition, why);
            if use_ledger && !opts.dry_run {
                let _ = self.ledger.finish(&a.id, Err(&e.to_string()));
            }
            return self.fail(&a, c.effect, e, attempts, opts);
        }
        if opts.dry_run {
            out.text =
                format!("[preview — niets toegepast]\n{}", out.preview.clone().unwrap_or_else(|| out.text.clone()));
        }
        if use_ledger && !opts.dry_run {
            let _ = self.ledger.finish(&a.id, Ok(&out));
        }
        if c.cacheable && c.effect == Effect::Read {
            self.cache.put(ikey, out.clone());
        }
        let ms = started.elapsed().as_millis() as u64;
        self.emit(
            EventKind::ToolCallCompleted,
            opts,
            serde_json::json!({
                "action_id": a.id, "tool": c.name, "ms": ms, "attempts": attempts, "chars": out.text.len(),
                "preview": out.preview.as_ref().map(|p| p.chars().take(4000).collect::<String>()),
                "output": out.text.chars().take(600).collect::<String>(), "dry_run": opts.dry_run
            }),
        );
        self.finish_ok(&a, &c.name, c.effect, out, attempts, false, started, opts)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_ok(
        &self,
        a: &Action,
        tool: &str,
        effect: Effect,
        out: ToolOutput,
        attempts: u32,
        cached: bool,
        started: Instant,
        opts: &ExecOptions,
    ) -> ActionResult {
        let mut out = if opts.filter.is_empty() { out } else { opts.filter.apply(out) };
        out.text = trim_middle(&out.text, opts.max_output_chars);
        ActionResult {
            action_id: a.id.clone(),
            tool: tool.into(),
            ok: true,
            output: Some(out),
            error: None,
            effect,
            attempts,
            cached,
            replayed: false,
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }
}
