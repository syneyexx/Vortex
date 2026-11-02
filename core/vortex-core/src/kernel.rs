//! De kern: bindt opslag, event log, runs, cancellation, budgetten, checkpoints, resources,
//! config, secrets, supervisie en de GPU-scheduler samen. Eigenaar van alle runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, Priority, RunId, RunState, StopStatus, TraceId};

use crate::budget::{Budget, BudgetTracker, Deadline};
use crate::checkpoint::{Checkpoint, Checkpoints, EffectKind};
use crate::config::{ConfigManager, ConfigSnapshot};
use crate::db::{CORE_MIGRATIONS, Db};
use crate::error::{CoreError, Result};
use crate::events::{Dispatcher, EventLog};
use crate::process::{ProcessOutcome, ProcessSpec, Supervisor};
use crate::recovery::{CrashReport, RunningMarker, recover};
use crate::resources::ResourceRegistry;
use crate::runs::{RunRecord, RunSpec, RunStore, TransitionData};
use crate::scheduler::{SlotPermit, SlotScheduler};
use crate::scope::DetachedTasks;
use crate::secrets::SecretBroker;
use crate::telemetry::TraceCtx;

/// Instellingen om de kern te openen.
#[derive(Debug, Clone)]
pub struct KernelOptions {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    /// Slots op de hoofd-GPU (standaard 1: één taak tegelijk).
    pub main_gpu_slots: usize,
    pub secrets: SecretBroker,
    /// Config hot-reload aan.
    pub watch_config: bool,
}

impl KernelOptions {
    pub fn new(data_dir: impl Into<PathBuf>, config_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            config_dir: config_dir.into(),
            main_gpu_slots: 1,
            secrets: SecretBroker::os(),
            watch_config: true,
        }
    }
}

#[derive(Debug)]
struct ActiveRun {
    token: CancellationToken,
}

#[derive(Debug)]
struct Inner {
    db: Arc<Db>,
    log: EventLog,
    runs: RunStore,
    checkpoints: Checkpoints,
    resources: ResourceRegistry,
    config: ConfigManager,
    secrets: SecretBroker,
    supervisor: Supervisor,
    main_gpu: SlotScheduler,
    detached: DetachedTasks,
    root: CancellationToken,
    active: Mutex<HashMap<RunId, ActiveRun>>,
    marker: RunningMarker,
}

/// Handle naar de kern (goedkoop te klonen).
#[derive(Debug, Clone)]
pub struct Kernel(Arc<Inner>);

impl Kernel {
    /// Open de kern. Voert eerst crash-herstel uit en geeft het rapport terug.
    pub async fn open(opts: KernelOptions) -> Result<(Kernel, CrashReport)> {
        let (marker, crashed) = RunningMarker::place(&opts.data_dir)?;
        let db = Arc::new(Db::open(opts.data_dir.join("vortex.db"), CORE_MIGRATIONS)?);
        let log = EventLog::new(db.clone());
        let runs = RunStore::new(log.clone());
        let checkpoints = Checkpoints::new(log.clone());
        let resources = ResourceRegistry::new(log.clone());
        let report = recover(&log, &runs, &checkpoints, &resources, crashed)?;
        let config = ConfigManager::load(&opts.config_dir, log.clone())?;
        let supervisor = Supervisor::new(log.clone(), resources.clone());
        let main_gpu = SlotScheduler::new("gpu.main", opts.main_gpu_slots, log.clone(), resources.clone());
        let root = CancellationToken::new();
        let detached = DetachedTasks::new();
        if opts.watch_config {
            let h = config.watch(root.child_token())?;
            detached.spawn("config-watcher", async move {
                let _ = h.await;
            });
        }
        log.append(Event::new(
            EventKind::SystemStarted,
            TraceId::new(),
            serde_json::json!({"crashed_before": crashed, "config_version": config.current().version, "pid": std::process::id()}),
        ))?;
        let k = Kernel(Arc::new(Inner {
            db,
            log,
            runs,
            checkpoints,
            resources,
            config,
            secrets: opts.secrets,
            supervisor,
            main_gpu,
            detached,
            root,
            active: Mutex::new(HashMap::new()),
            marker,
        }));
        if crashed {
            tracing::warn!("{}", report.summary());
        }
        Ok((k, report))
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.0.db
    }
    pub fn events(&self) -> &EventLog {
        &self.0.log
    }
    pub fn runs(&self) -> &RunStore {
        &self.0.runs
    }
    pub fn checkpoints(&self) -> &Checkpoints {
        &self.0.checkpoints
    }
    pub fn resources(&self) -> &ResourceRegistry {
        &self.0.resources
    }
    pub fn config(&self) -> &ConfigManager {
        &self.0.config
    }
    pub fn secrets(&self) -> &SecretBroker {
        &self.0.secrets
    }
    pub fn supervisor(&self) -> &Supervisor {
        &self.0.supervisor
    }
    pub fn main_gpu(&self) -> &SlotScheduler {
        &self.0.main_gpu
    }
    pub fn detached(&self) -> &DetachedTasks {
        &self.0.detached
    }
    /// Dispatcher voor event-ontvangers (start met `.spawn(...)`).
    pub fn dispatcher(&self) -> Dispatcher {
        Dispatcher::new(self.0.log.clone())
    }
    /// Token dat afgaat bij afsluiten.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.0.root.clone()
    }

    /// Nieuwe run aanmaken (status `queued`) en een context teruggeven.
    pub fn start_run(&self, spec: RunSpec) -> Result<RunContext> {
        let parent_token = spec.parent.and_then(|p| self.0.active.lock().get(&p).map(|a| a.token.clone()));
        let token = parent_token.unwrap_or_else(|| self.0.root.clone()).child_token();
        let config = self.0.config.current();
        let rec = self.0.runs.create(&spec, Some(&config.version))?;
        let deadline = rec.deadline.map(|d| {
            let left = (d - vortex_types::now()).to_std().unwrap_or(Duration::ZERO);
            Deadline::after(left)
        });
        let budget = BudgetTracker::new(spec.budget.clone(), deadline.unwrap_or(Deadline::NONE));
        self.0.active.lock().insert(rec.id, ActiveRun { token: token.clone() });
        Ok(RunContext {
            kernel: self.clone(),
            trace: TraceCtx { trace_id: rec.trace_id, span_id: vortex_types::SpanId::new(), run_id: Some(rec.id) },
            record: Mutex::new(rec),
            token,
            budget,
            config,
            finished: Mutex::new(false),
        })
    }

    /// Hervat een run die na een crash terug in de wachtrij staat.
    pub fn resume_run(&self, id: RunId) -> Result<(RunContext, Option<Checkpoint>)> {
        let rec = self.0.runs.get(id)?;
        if rec.state != RunState::Queued {
            return Err(CoreError::InvalidTransition {
                from: rec.state,
                to: RunState::Starting,
                reason: "alleen queued runs hervatten".into(),
            });
        }
        let token = self.0.root.child_token();
        let config = self.0.config.current();
        let budget = BudgetTracker::new(rec.budget.clone(), Deadline::NONE);
        self.0.active.lock().insert(id, ActiveRun { token: token.clone() });
        let ckp = self.0.checkpoints.latest(id)?;
        Ok((
            RunContext {
                kernel: self.clone(),
                trace: TraceCtx { trace_id: rec.trace_id, span_id: vortex_types::SpanId::new(), run_id: Some(id) },
                record: Mutex::new(rec),
                token,
                budget,
                config,
                finished: Mutex::new(false),
            },
            ckp,
        ))
    }

    /// Stopknop: UI → kern → run → subtaken → tools → processen. Geeft `true` als er iets te stoppen was.
    pub fn request_stop(&self, id: RunId) -> Result<bool> {
        let rec = self.0.runs.get(id)?;
        if rec.state.is_terminal() {
            return Ok(false);
        }
        self.0.runs.set_stop_status(id, StopStatus::Requested)?;
        match rec.state {
            RunState::Queued => {
                self.0.runs.set_stop_status(id, StopStatus::Stopped)?;
                self.0.runs.transition(
                    id,
                    RunState::Cancelled,
                    None,
                    TransitionData { reason: Some("gestopt in wachtrij".into()), ..Default::default() },
                )?;
                self.0.runs.set_stop_status(id, StopStatus::CleanedUp)?;
            }
            RunState::Cancelling => {}
            _ => {
                self.0.runs.transition(
                    id,
                    RunState::Cancelling,
                    None,
                    TransitionData { reason: Some("stop aangevraagd".into()), ..Default::default() },
                )?;
            }
        }
        // Kinderen eerst (zelfde keten).
        for child in self.0.runs.children(id)? {
            if !child.state.is_terminal() {
                self.request_stop(child.id)?;
            }
        }
        if let Some(a) = self.0.active.lock().get(&id) {
            a.token.cancel();
        }
        Ok(true)
    }

    /// Nette afsluiting: alles stoppen, marker weg.
    pub async fn shutdown(&self, grace: Duration) -> Result<()> {
        let ids: Vec<RunId> = self.0.active.lock().keys().copied().collect();
        for id in ids {
            let _ = self.request_stop(id);
        }
        self.0.root.cancel();
        let deadline = tokio::time::Instant::now() + grace;
        while !self.0.active.lock().is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.0.detached.abort_all();
        self.0.db.checkpoint_wal()?;
        self.0.marker.clear()?;
        Ok(())
    }

    fn forget(&self, id: RunId) {
        self.0.active.lock().remove(&id);
    }
}

/// Alles wat een lopende run nodig heeft. Afronden via `complete`/`fail`/`finish_cancelled`;
/// wordt de context gedropt zonder afronding, dan rondt hij zichzelf veilig af.
#[derive(Debug)]
pub struct RunContext {
    kernel: Kernel,
    trace: TraceCtx,
    record: Mutex<RunRecord>,
    token: CancellationToken,
    budget: BudgetTracker,
    config: Arc<ConfigSnapshot>,
    finished: Mutex<bool>,
}

impl RunContext {
    pub fn id(&self) -> RunId {
        self.record.lock().id
    }
    pub fn trace(&self) -> TraceCtx {
        self.trace
    }
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
    pub fn budget(&self) -> &BudgetTracker {
        &self.budget
    }
    /// De config-versie van deze run (blijft gelijk tot de run klaar is).
    pub fn config(&self) -> &Arc<ConfigSnapshot> {
        &self.config
    }
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }
    pub fn state(&self) -> RunState {
        self.record.lock().state
    }
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    /// Gebruik deze run-versie van de config i.p.v. de nieuwste ("nu toepassen" = nieuwe versie pakken).
    pub fn apply_latest_config(&mut self) {
        self.config = self.kernel.config().current();
    }

    fn go(&self, to: RunState, data: TransitionData) -> Result<()> {
        let mut rec = self.record.lock();
        let new = self.kernel.runs().transition(rec.id, to, Some(rec.version), data)?;
        *rec = new;
        Ok(())
    }

    fn refresh(&self) -> Result<()> {
        let id = self.id();
        *self.record.lock() = self.kernel.runs().get(id)?;
        Ok(())
    }

    /// queued → starting → running.
    pub fn start(&self) -> Result<()> {
        self.ensure_not_cancelled()?;
        self.go(RunState::Starting, TransitionData::default())?;
        self.go(RunState::Running, TransitionData::default())
    }

    /// running → waiting (bv. wacht op GPU-slot of gebruiker).
    pub fn wait(&self, reason: &str) -> Result<()> {
        self.go(RunState::Waiting, TransitionData { reason: Some(reason.into()), ..Default::default() })
    }

    /// waiting → running.
    pub fn resume(&self) -> Result<()> {
        self.ensure_not_cancelled()?;
        self.go(RunState::Running, TransitionData::default())
    }

    fn ensure_not_cancelled(&self) -> Result<()> {
        if self.token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        Ok(())
    }

    /// Eén stap: controleert stopverzoek, deadline en budget.
    pub fn step(&self) -> Result<()> {
        self.ensure_not_cancelled()?;
        let r = self.budget.charge_step();
        self.persist_usage();
        self.budget_event(&r);
        r
    }

    /// Tokens verbruiken.
    pub fn charge_tokens(&self, n: u64) -> Result<()> {
        self.ensure_not_cancelled()?;
        let r = self.budget.charge_tokens(n);
        self.persist_usage();
        self.budget_event(&r);
        r
    }

    fn persist_usage(&self) {
        let _ = self.kernel.runs().record_usage(self.id(), &self.budget.usage());
    }

    fn budget_event(&self, r: &Result<()>) {
        let kind = match r {
            Err(CoreError::BudgetExceeded(_)) => EventKind::RunBudgetExceeded,
            Err(CoreError::DeadlineExceeded) => EventKind::RunDeadlineExceeded,
            _ => return,
        };
        let _ = self.kernel.events().append(
            Event::new(
                kind,
                self.trace.trace_id,
                serde_json::json!({"usage": self.budget.usage(), "error": r.as_ref().err().map(|e| e.to_string())}),
            )
            .for_run(self.id()),
        );
    }

    /// Checkpoint opslaan (hervatpunt).
    pub fn checkpoint(&self, data: &serde_json::Value) -> Result<i64> {
        self.kernel.checkpoints().save(self.id(), data)
    }

    /// Effect uitvoeren met registratie (intentie vóór, status na). Voor externe acties verplicht.
    pub async fn effect<T, F, Fut>(
        &self,
        kind: EffectKind,
        description: &str,
        idempotency_key: Option<&str>,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let id = self.kernel.checkpoints().effect_intent(self.id(), kind, description, idempotency_key)?;
        let r = f().await;
        self.kernel.checkpoints().effect_finished(&id, r.is_ok())?;
        r
    }

    /// Wacht op het GPU-slot (status waiting zolang er gewacht wordt).
    pub async fn gpu(&self, priority: Priority) -> Result<SlotPermit> {
        let sched = self.kernel.main_gpu().clone();
        let id = self.id();
        if sched.queue_len() > 0 {
            self.wait("wacht op GPU-slot")?;
            let p = sched.acquire(id, priority, &self.token).await;
            self.resume()?;
            return p;
        }
        sched.acquire(id, priority, &self.token).await
    }

    /// Proces starten onder supervisie (stopt mee met deze run).
    pub async fn spawn_process(&self, spec: ProcessSpec) -> Result<ProcessOutcome> {
        self.kernel.supervisor().run(spec, Some(self.id()), self.trace.trace_id, self.token.child_token()).await
    }

    /// Subrun binnen deze run (erft trace, deadline en resterend budget; stopt mee).
    pub fn child(&self, mut spec: RunSpec, own_budget: Budget) -> Result<RunContext> {
        spec.parent = Some(self.id());
        spec.trace_id = Some(self.trace.trace_id);
        let mut ctx = self.kernel.start_run(spec)?;
        ctx.budget = self.budget.child(own_budget);
        ctx.config = self.config.clone();
        Ok(ctx)
    }

    /// Afronden met definitieve uitvoer (eerst opslaan, dan pas `completed`).
    pub fn complete(self, output: serde_json::Value) -> Result<RunRecord> {
        if self.token.is_cancelled() {
            self.finish_cancelled_inner()?;
            return Err(CoreError::Cancelled);
        }
        self.go(RunState::Completed, TransitionData { output: Some(output), ..Default::default() })?;
        self.cleanup();
        self.kernel.runs().get(self.id())
    }

    /// Afronden als mislukt.
    pub fn fail(self, error: &str) -> Result<RunRecord> {
        self.fail_inner(error)?;
        self.kernel.runs().get(self.id())
    }

    fn fail_inner(&self, error: &str) -> Result<()> {
        self.refresh()?;
        if !self.state().is_terminal() {
            self.go(RunState::Failed, TransitionData { error: Some(error.into()), ..Default::default() })?;
        }
        self.cleanup();
        Ok(())
    }

    /// Na stopverzoek: werk is gestopt → opruimen → `cancelled`.
    pub fn finish_cancelled(self) -> Result<RunRecord> {
        self.finish_cancelled_inner()?;
        self.kernel.runs().get(self.id())
    }

    fn finish_cancelled_inner(&self) -> Result<()> {
        let id = self.id();
        self.refresh()?;
        if self.state().is_terminal() {
            self.cleanup();
            return Ok(());
        }
        if self.state() != RunState::Cancelling {
            // Stop kwam via ouder/afsluiten: alsnog via de kern laten lopen.
            self.kernel.request_stop(id)?;
            self.refresh()?;
        }
        self.kernel.runs().set_stop_status(id, StopStatus::Stopped)?;
        self.refresh()?;
        if !self.state().is_terminal() {
            self.go(RunState::Cancelled, TransitionData { reason: Some("gestopt".into()), ..Default::default() })?;
        }
        self.cleanup();
        self.kernel.runs().set_stop_status(id, StopStatus::CleanedUp)?;
        Ok(())
    }

    fn cleanup(&self) {
        let id = self.id();
        *self.finished.lock() = true;
        self.token.cancel(); // subtaken stoppen mee
        if let Err(e) = self.kernel.resources().release_run(id) {
            tracing::error!(run = %id, error = %e, "resources vrijgeven mislukt");
        }
        self.kernel.forget(id);
    }
}

impl Drop for RunContext {
    fn drop(&mut self) {
        if *self.finished.lock() {
            return;
        }
        let r = if self.token.is_cancelled() {
            self.finish_cancelled_inner()
        } else {
            self.fail_inner("run-context zonder afronding beëindigd (panic of programmeerfout)")
        };
        if let Err(e) = r {
            tracing::error!(run = %self.id(), error = %e, "automatisch afronden van run mislukt");
            self.cleanup();
        }
    }
}
