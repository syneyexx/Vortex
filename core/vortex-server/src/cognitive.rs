//! De chat als cognitieve cyclus (W07): waarnemen → aandacht → herinneren → overwegen
//! (router + metacognitie) → handelen (snel of plan/reflexion) → verifiëren → reflecteren
//! (leeragenda) → consolideren (geheugen). Elke fase is een event; de Global Workspace is het
//! schoolbord; bijsturen kan tijdens het streamen.
// VX:W07.01

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use vortex_cognition::planning::{critique, execute_plan, make_plan};
use vortex_cognition::{
    CogLlm, Cycle, GenOpts, Path, Phase, PlanStyle, Prediction, Signals, UserProfile, Workspace, decide, salience,
};
use vortex_context::{AssemblyInput, Budget, MemItem, ToolBrief, Turn, assemble};
use vortex_core::{CoreError, RunContext};
use vortex_models::backend::{Backend, ChatRequest, Message, ResponseFormat, Role, StreamEvent, ToolCall, ToolSpec};
use vortex_models::{ModelError, OpenAiBackend};
use vortex_tools::skills::propose::Step;
use vortex_tools::{Action, ExecOptions, ToolHub};
use vortex_types::{DecisionRecord, Event, EventId, EventKind, MessageId, Priority, RunId, SessionId};

use crate::chat::{COALESCE_CHARS, COALESCE_MS, ChatEngine, ChatError, MEMORY_TIMEOUT_MS, Mode};
use crate::sessions::SessionStore;

/// `CogLlm` via een backend (niet-streamend, voor plan/stappen/kritiek).
#[derive(Debug)]
pub struct BackendLlm {
    pub backend: Arc<OpenAiBackend>,
    pub cancel: CancellationToken,
}

#[async_trait::async_trait]
impl CogLlm for BackendLlm {
    async fn complete(&self, messages: Vec<Message>, opts: GenOpts) -> Result<String, String> {
        let mut req = ChatRequest::new(messages);
        req.max_tokens = opts.max_tokens;
        req.temperature = opts.temperature;
        if self.backend.capabilities().json_schema
            && let Some(s) = opts.json_schema
        {
            req.response_format = Some(ResponseFormat::JsonSchema { name: "out".into(), schema: s });
        }
        if self.backend.capabilities().reasoning_effort {
            req.reasoning_effort = opts.reasoning_effort;
        }
        self.backend.chat(&req, &self.cancel).await.map(|r| r.content).map_err(|e| e.to_string())
    }
}

/// Eenvoudige taalcheck: is het antwoord vooral Engels terwijl de vraag Nederlands is?
fn mostly_english(q: &str, a: &str) -> bool {
    let nl = |t: &str| {
        ["de ", "het ", "een ", "is ", "en ", "van ", "ik ", "je ", "wat ", "hoe "]
            .iter()
            .filter(|w| t.to_lowercase().contains(*w))
            .count()
    };
    let en = |t: &str| {
        ["the ", " is ", " and ", " of ", " you ", " what ", " how ", " this "]
            .iter()
            .filter(|w| t.to_lowercase().contains(*w))
            .count()
    };
    nl(q) >= 2 && en(a) > nl(a) + 2 && a.len() > 80
}

fn admits_unknown(a: &str) -> bool {
    let l = a.to_lowercase();
    ["weet ik niet", "weet het niet", "niet zeker", "geen informatie", "kan ik niet", "heb ik geen", "niet bekend"]
        .iter()
        .any(|k| l.contains(k))
}

impl ChatEngine {
    /// Embedding-gelijkenis vraag ⇄ skillbeschrijving (0..1), met cache. Kort timeout: de
    /// chat wacht hier nooit lang op.
    async fn skill_semantic(&self, h: &ToolHub, q: &str) -> std::collections::HashMap<String, f64> {
        let mut out = std::collections::HashMap::new();
        let Some(mem) = &self.memory else { return out };
        let skills = h.skills.list();
        if skills.is_empty() {
            return out;
        }
        let emb = mem.store.embedder();
        let key = |s: &vortex_tools::Skill| format!("{}|{}", s.name, s.description);
        let need: Vec<(String, String)> = {
            let c = self.skill_vecs.lock();
            skills
                .iter()
                .filter(|s| !c.contains_key(&key(s)))
                .map(|s| (key(s), format!("{}: {}", s.name.replace('-', " "), s.description)))
                .collect()
        };
        if !need.is_empty() {
            let texts: Vec<String> = need.iter().map(|(_, t)| t.clone()).collect();
            if let Ok(Ok(vs)) = tokio::time::timeout(Duration::from_millis(1500), emb.embed(&texts)).await {
                let mut c = self.skill_vecs.lock();
                for ((k, _), v) in need.into_iter().zip(vs) {
                    c.insert(k, v);
                }
            }
        }
        let Ok(Ok(qv)) = tokio::time::timeout(Duration::from_millis(800), emb.embed(&[q.to_string()])).await else {
            return out;
        };
        let Some(qv) = qv.into_iter().next() else { return out };
        let c = self.skill_vecs.lock();
        for s in &skills {
            if let Some(v) = c.get(&key(s)) {
                let dot: f32 = v.iter().zip(&qv).map(|(a, b)| a * b).sum();
                let na: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                let nb: f32 = qv.iter().map(|x| x * x).sum::<f32>().sqrt();
                if na > 0.0 && nb > 0.0 {
                    out.insert(s.name.clone(), f64::from((dot / (na * nb)).max(0.0)));
                }
            }
        }
        out
    }

    /// Nieuwste samenvatting van deze sessie uit het geheugen (W05 `summarize_session`).
    fn session_summary(&self, session: SessionId) -> Option<String> {
        let mem = self.memory.as_ref()?;
        let origin = format!("sessie:{session}");
        let f = vortex_memory::Filters {
            kinds: vec![vortex_memory::MemoryKind::Summary],
            include_stale: true,
            ..Default::default()
        };
        mem.store
            .list(&f, None, 200, 0)
            .ok()?
            .into_iter()
            .filter(|m| m.provenance.origin.as_deref() == Some(origin.as_str()))
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|m| m.content)
    }

    /// Bijsturing doorgeven aan een lopende run.
    pub fn steer(&self, run: RunId, text: &str) -> bool {
        let tx = self.steer.lock().get(&run).cloned();
        if let Some(ws) = self.workspaces.lock().get(&run) {
            ws.steer(text);
        }
        match tx {
            Some(t) => t.try_send(text.to_string()).is_ok(),
            None => false,
        }
    }

    /// Momentopname van de workspace van een (lopende) run.
    pub fn workspace(&self, run: RunId) -> Option<vortex_cognition::workspace::Board> {
        self.workspaces.lock().get(&run).map(|w| w.snapshot())
    }

    pub fn agenda(&self) -> &vortex_cognition::Agenda {
        &self.agenda
    }

    fn history_turns(&self, session: SessionId, upto: MessageId) -> Result<Vec<Turn>, ChatError> {
        let mut out = Vec::new();
        for m in self.sessions.messages(session)? {
            if m.id == upto {
                break;
            }
            if m.content.is_empty() {
                continue;
            }
            match m.role.as_str() {
                "user" => out.push(Turn { role: Role::User, content: m.content, tool_ref: None }),
                "assistant" if m.status == "complete" => {
                    out.push(Turn { role: Role::Assistant, content: m.content, tool_ref: None })
                }
                _ => {}
            }
        }
        // De laatste "user" is de huidige vraag; die gaat apart mee.
        if out.last().is_some_and(|t| t.role == Role::User) {
            out.pop();
        }
        Ok(out)
    }

    pub(crate) async fn run(
        &self,
        ctx: RunContext,
        backend: Arc<OpenAiBackend>,
        session: SessionId,
        msg: MessageId,
        mode: Mode,
    ) {
        let run_id = ctx.id();
        let trace = ctx.trace().trace_id;
        let log = self.kernel.events().clone();
        let (steer_tx, mut steer_rx) = tokio::sync::mpsc::channel::<String>(8);
        self.steer.lock().insert(run_id, steer_tx);
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut cyc = Cycle::new(run_id, trace, log.clone());
        let question = self
            .sessions
            .messages(session)
            .ok()
            .and_then(|ms| ms.into_iter().rev().find(|m| m.role == "user").map(|m| m.content))
            .unwrap_or_default();
        let ws = Workspace::new(run_id, trace, &question, Some(log.clone()));
        self.workspaces.lock().insert(run_id, ws.clone());

        // Tools (W08): stappen voor skill-voorstellen, gebruikte skills, verificatie-uitkomst.
        let mut tool_steps: Vec<Step> = Vec::new();
        let mut used_skills: Vec<String> = Vec::new();
        let mut verified_ok = false;
        let outcome: Result<Option<serde_json::Value>, ChatError> = async {
            ctx.start()?;
            // 1. Waarnemen
            cyc.enter(Phase::Perceive);
            let mut route = vortex_cognition::router::route_with(&question, &self.scorer, &run_id.to_string());
            // Twijfel → klein routermodel labelt (max 1,5 s; anders blijft de heuristiek staan).
            if route.confidence < vortex_cognition::router::LM_LABEL_BELOW {
                let reg = vortex_models::Registry::from_config(ctx.config()).ok();
                let rb = reg.as_ref().and_then(|r| r.for_role("router").map(|(n, _)| n.to_string()).and_then(|n| self.backends.for_model(r, &n).ok()));
                if let Some(rb) = rb {
                    let rl = BackendLlm { backend: rb, cancel: ctx.token().child_token() };
                    if let Ok(r) = tokio::time::timeout(Duration::from_millis(1500), vortex_cognition::router::refine_with_lm(&rl, &question, route.clone())).await {
                        route = r;
                    }
                }
            }
            cyc.finish(serde_json::json!({"task": route.task, "difficulty": route.difficulty, "confidence": route.confidence}));

            // 2+3. Herinneren (vóór aandacht: nieuwheid hangt af van het geheugen)
            cyc.enter(Phase::Recall);
            let mut mems: Vec<MemItem> = Vec::new();
            let mut best: Option<f64> = None;
            let mut conflicts = 0usize;
            if let Some(mem) = &self.memory {
                let mut q = vortex_memory::Query::new(question.clone());
                q.k = 6;
                q.run_id = Some(run_id);
                if route.task == vortex_cognition::TaskType::Personal {
                    q.prefer_kinds = vec![vortex_memory::MemoryKind::User, vortex_memory::MemoryKind::Project];
                }
                if let Ok(Ok(items)) = tokio::time::timeout(Duration::from_millis(MEMORY_TIMEOUT_MS), mem.store.retrieve(&q, mem.reranker.as_ref())).await {
                    best = items.iter().filter(|i| !i.counter_evidence).map(|i| i.score.final_score).reduce(f64::max);
                    conflicts = items.iter().filter(|i| i.counter_evidence).count();
                    ws.set_memories(items.iter().map(|i| i.memory.id.to_string()).collect());
                    mems = items
                        .into_iter()
                        .map(|it| {
                            let m = it.memory;
                            MemItem {
                                id: m.id.to_string(),
                                kind: m.kind.as_str().into(),
                                text: m.content.chars().take(800).collect(),
                                source: m.source_uri.clone().or(m.provenance.origin.clone()).unwrap_or_else(|| "geheugen".into()),
                                date: m.event_time.clone().unwrap_or(m.created_at.clone()).chars().take(10).collect(),
                                score: it.score.final_score,
                                counter: it.counter_evidence,
                            }
                        })
                        .collect();
                }
            }
            cyc.finish(serde_json::json!({"memories": mems.len(), "best": best, "conflicts": conflicts}));

            cyc.enter(Phase::Attend);
            let sal = salience::tag_with(&question, best, conflicts, &self.scorer, &run_id.to_string());
            let _ = log.append(Event::new(EventKind::SalienceTagged, trace, serde_json::to_value(&sal).unwrap_or_default()).for_run(run_id));
            cyc.finish(serde_json::json!({"urgent": sal.urgent, "novelty": sal.novelty}));

            // 4. Overwegen: metacognitie kiest pad + strategie.
            cyc.enter(Phase::Deliberate);
            let strategy = decide(&Signals {
                route: route.clone(),
                mode: mode.as_str().into(),
                best_memory: best,
                conflicts,
                tools: crate::eval_api::FEATURES.contains(&"W08"),
                research: crate::eval_api::FEATURES.contains(&"W10"),
            });
            let _ = log.log_decision(
                trace,
                Some(run_id),
                DecisionRecord {
                    decision_id: EventId::new(),
                    decider: "router+metacog".into(),
                    decider_version: "heuristiek-v1".into(),
                    features: serde_json::to_value(&route.features).unwrap_or_default(),
                    choice: serde_json::json!({"task": route.task, "path": strategy.path, "plan": strategy.plan}),
                    alternatives: vec![],
                    confidence: Some(route.confidence),
                    outcome: None,
                },
            );
            let sv = serde_json::to_value(&strategy).unwrap_or_default();
            ws.set_strategy(sv.clone());
            let _ = log.append(Event::new(EventKind::StrategyChosen, trace, sv).for_run(run_id));
            cyc.finish(serde_json::json!({"path": strategy.path, "plan": strategy.plan, "prediction": strategy.prediction}));

            // 5. Handelen
            cyc.enter(Phase::Act);
            let profile = self.memory.as_ref().map(|m| UserProfile::build(&m.store, None)).and_then(|p| p.prompt_block());
            let mut sys = ctx.config().file("prompts/system.md").map(|s| s.trim().to_string()).unwrap_or_default();
            let mut guidance = strategy.guidance.clone();
            // Tools + skills (W08): kernset met schema; rest via discovery; passende skills laden.
            let hub = self.tools.clone().filter(|_| crate::eval_api::FEATURES.contains(&"W08") && backend.capabilities().tools);
            let mut briefs: Vec<ToolBrief> = Vec::new();
            if let Some(h) = &hub {
                let sem = self.skill_semantic(h, &question).await;
                for sel in h.skills.select(&question, &[], &sem, 3) {
                    if sel.score >= 0.6
                        && let Some(sk) = h.skills.get(&sel.name)
                    {
                        h.skills.record_use(&sk.name);
                        used_skills.push(sk.name.clone());
                        guidance.push(format!("Actieve skill '{}' ({}):\n{}", sk.name, sel.reason, sk.body.trim()));
                    }
                }
                let core = h.core_set();
                briefs = h.registry.briefs().into_iter().filter(|b| !core.contains(&b.name)).map(|b| ToolBrief { name: b.name, description: b.description }).collect();
                let sk = h.skills.briefs_text();
                if !sk.is_empty() {
                    sys.push_str("\n\nSkills (laad de instructies met load_skill als er een past):\n");
                    sys.push_str(&sk);
                }
                sys.push_str("\n\nJe kunt tools gebruiken. Doe dat als het antwoord daardoor beter of controleerbaar wordt (bestanden, berekeningen, zoeken, geheugen). Tools uit de lijst 'Beschikbare tools' roep je aan via call_tool (schema eerst met describe_tool). Na een fout: lees de melding en pas je aanpak aan.");
            }
            let llm = BackendLlm { backend: backend.clone(), cancel: ctx.token().child_token() };
            let _gpu = ctx.gpu(Priority::P0).await?;
            let w09 = crate::eval_api::FEATURES.contains(&"W09");
            // Conflict-team op de voorgrond: botsing in wat we net ophaalden → eerst beslissen (W09.09).
            if w09
                && conflicts > 0
                && let Some(mem) = &self.memory
            {
                let ids: Vec<String> = mems.iter().map(|m| m.id.clone()).collect();
                let open: Vec<vortex_memory::Conflict> = mem
                    .store
                    .conflicts(Some("open"))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| ids.contains(&c.a.to_string()) || ids.contains(&c.b.to_string()))
                    .take(2)
                    .collect();
                let deps = vortex_agents::conflict::ConflictDeps {
                    llm: Arc::new(BackendLlm { backend: backend.clone(), cancel: ctx.token().child_token() }),
                    roles: vortex_agents::Roles::from_snapshot(ctx.config()),
                    reporter: vortex_agents::Reporter::new(Some(log.clone()), Some(run_id), trace),
                };
                for c in &open {
                    if let Ok(Ok(r)) = tokio::time::timeout(Duration::from_secs(120), vortex_agents::conflict::resolve(&deps, &mem.store, c)).await {
                        guidance.push(format!("Een tegenstrijdigheid in het geheugen is net door het conflict-team beslist: {}", serde_json::to_string(&r).unwrap_or_default()));
                    }
                }
            }
            // Research-pad (W10): research-team → rapport met bronverwijzingen → Synthesizer.
            let research = strategy.path == Path::Research && crate::eval_api::FEATURES.contains(&"W10");
            if research {
                let rmode = vortex_agents::research::choose_mode(mode.as_str(), route.difficulty, route.features.words);
                let rid = format!("res_{}", run_id.to_string().trim_start_matches("run_"));
                let out = self.research_in(&ctx, &rid, &question, rmode, None, Some(ws.clone())).await;
                let _ = ctx.checkpoint(&serde_json::json!({"research": rid, "sources": out.sources.len(), "records": out.records}));
                guidance.push(format!(
                    "Onderzoeksrapport van het research-team ({} modus, {} bronnen, {} records, {}):\n{}\n\nGeef de gebruiker een helder antwoord op basis van dit rapport. Behoud de [n]-bronverwijzingen en sluit af met de bronnenlijst. Voeg niets toe wat niet in het rapport staat.{}",
                    out.mode,
                    out.sources.len(),
                    out.records,
                    if out.scope == "lokaal" { "alleen lokaal — geen internet" } else { "lokaal + web" },
                    out.report.chars().take(12_000).collect::<String>(),
                    out.report_path.as_ref().map(|p| format!(" Het volledige rapport staat in {p}.")).unwrap_or_default()
                ));
            }
            let team = if w09 && !research && strategy.prediction != Prediction::AskUser { vortex_agents::compose(&route, &strategy, mode.as_str()) } else { None };
            if let Some(t) = &team {
                // Debatteam (W09): pas antwoorden als de Judge het vertrouwt; daarna streamt de Synthesizer.
                let mut material: Vec<String> = mems.iter().map(|m| format!("[{} {}] {}", m.kind, m.date, m.text)).collect();
                material.extend(guidance.iter().cloned());
                let deps = vortex_agents::DebateDeps {
                    main: Arc::new(BackendLlm { backend: backend.clone(), cancel: ctx.token().child_token() }),
                    aux: self.role_llm(ctx.config(), "extract", ctx.token().child_token()),
                    extra: vec![],
                    memory: self.memory.as_ref().map(|m| (m.store.clone(), m.reranker.clone())),
                    tools: self.tools.clone(),
                    online: self.online.online().await,
                    roles: vortex_agents::Roles::from_snapshot(ctx.config()),
                    reporter: vortex_agents::Reporter::new(Some(log.clone()), Some(run_id), trace),
                    exec: ExecOptions { run: Some(run_id), trace, cancel: ctx.token().child_token(), ..Default::default() },
                    workspace: Some(ws.clone()),
                    material,
                };
                let out = vortex_agents::debate::run(&deps, &question, t).await;
                let _ = ctx.checkpoint(&serde_json::json!({"team": t.roles(), "judge": out.verdict, "trusted": out.trusted}));
                guidance.push(vortex_agents::debate::synthesizer_guidance(&deps.roles, &out));
                deps.reporter.started("Synthesizer", "eindantwoord in jouw stijl");
            } else if !research && strategy.path != Path::Fast && strategy.prediction != Prediction::AskUser {
                // System 2: plan → stappen (geïsoleerde contexten) → evt. kritiek.
                let plan = make_plan(&llm, &ws, &question, &mems.iter().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n")).await;
                let _ = log.append(Event::new(EventKind::PlanCreated, trace, serde_json::json!({"steps": plan.steps})).for_run(run_id));
                let results = execute_plan(&llm, &ws, &question, strategy.max_steps, |i, step, out| {
                    let _ = log.append(
                        Event::new(EventKind::PlanStepCompleted, trace, serde_json::json!({"i": i, "step": step, "chars": out.len()})).for_run(run_id),
                    );
                    let _ = ctx.checkpoint(&serde_json::json!({"plan_step": i, "step": step}));
                })
                .await;
                let mut worked = String::from("Uitgewerkte stappen (gebruik dit als basis voor je antwoord; herhaal ze niet letterlijk):\n");
                for (s, r) in &results {
                    worked.push_str(&format!("## {s}\n{r}\n"));
                }
                if strategy.plan == PlanStyle::Reflexion {
                    let crit = critique(&llm, &question, &worked).await;
                    if crit.verdict == "revise" && !crit.issues.is_empty() {
                        worked.push_str("\nKritiekpunten om in je antwoord te herstellen:\n");
                        for i in &crit.issues {
                            worked.push_str(&format!("- {i}\n"));
                        }
                        ws.add_note(&format!("kritiek: {}", crit.issues.join("; ")));
                    }
                }
                guidance.push(worked);
            }
            let history = self.history_turns(session, msg)?;
            // Hiërarchisch: lange gesprekken krijgen de lopende sessiesamenvatting bovenop de beurt-gists.
            let rolling_summary = if history.len() > 8 { self.session_summary(session) } else { None };
            let input = AssemblyInput {
                rolling_summary,
                system: sys,
                user_profile: profile,
                memories: mems.clone(),
                history,
                user: question.clone(),
                guidance,
                tools: briefs,
                ..Default::default()
            };
            let assembled = assemble(&input, &Budget::default());
            let used: Vec<serde_json::Value> = mems.iter().map(|m| serde_json::json!({"id": m.id, "kind": m.kind, "score": m.score, "counter": m.counter})).collect();
            self.log_context(run_id, trace, &ctx, &assembled.messages, &used, assembled.report.dropped.len());
            let _ = log.append(
                Event::new(EventKind::ContextAssembled, trace, serde_json::json!({"report": assembled.report})).for_run(run_id),
            );
            let metrics;
            if let (Prediction::AskUser, Some(ask)) = (strategy.prediction, &strategy.ask) {
                // Vraag terug i.p.v. gokken.
                text.push_str(ask);
                log.append(
                    Event::new(EventKind::MessageDelta, trace, serde_json::json!({"session_id": session, "message_id": msg, "text": ask, "reasoning": ""})).for_run(run_id),
                )?;
                metrics = None;
            } else {
                let mut messages = assembled.messages;
                let effort = strategy.reasoning_effort.clone();
                const MAX_TOOL_ROUNDS: usize = 8;
                let mut tool_rounds = 0usize;
                loop {
                    let mut req = ChatRequest::new(messages.clone());
                    if backend.capabilities().reasoning_effort {
                        req.reasoning_effort = Some(effort.clone());
                    }
                    if let Some(h) = &hub
                        && tool_rounds < MAX_TOOL_ROUNDS
                    {
                        req.tools = h.specs().into_iter().map(|(name, description, parameters)| ToolSpec { name, description, parameters }).collect();
                    }
                    let mut calls: Vec<ToolCall> = Vec::new();
                    let gen_token = ctx.token().child_token();
                    let mut stream = backend.chat_stream(&req, gen_token.clone()).await?;
                    let (mut buf_t, mut buf_r) = (String::new(), String::new());
                    let mut last = Instant::now();
                    let mut got_metrics = None;
                    let mut steered: Option<String> = None;
                    let start_len = text.len();
                    let flush = |t: &mut String, r: &mut String| -> Result<(), ChatError> {
                        if t.is_empty() && r.is_empty() {
                            return Ok(());
                        }
                        log.append(
                            Event::new(EventKind::MessageDelta, trace, serde_json::json!({"session_id": session, "message_id": msg, "text": t, "reasoning": r}))
                                .for_run(run_id),
                        )?;
                        t.clear();
                        r.clear();
                        Ok(())
                    };
                    loop {
                        tokio::select! {
                            s = steer_rx.recv() => {
                                if let Some(s) = s {
                                    gen_token.cancel();
                                    steered = Some(s);
                                    break;
                                }
                            }
                            next = tokio::time::timeout(Duration::from_millis(COALESCE_MS), stream.next()) => {
                                match next {
                                    Err(_) => {}
                                    Ok(None) => break,
                                    Ok(Some(ev)) => match ev {
                                        Ok(StreamEvent::Text(t)) => { text.push_str(&t); buf_t.push_str(&t); }
                                        Ok(StreamEvent::Reasoning(r)) => { reasoning.push_str(&r); buf_r.push_str(&r); }
                                        Ok(StreamEvent::Done { metrics: m, usage, finish_reason }) => {
                                            let _ = ctx.charge_tokens(usage.completion_tokens);
                                            log.append(m.to_event(trace, Some(run_id)))?;
                                            got_metrics = Some(serde_json::json!({"metrics": m, "usage": usage, "finish_reason": finish_reason, "strategy": strategy.path}));
                                        }
                                        Ok(StreamEvent::ToolCallReady(c)) => calls.push(c),
                                        Ok(_) => {}
                                        Err(e) => return Err(e.into()),
                                    },
                                }
                            }
                        }
                        if last.elapsed() >= Duration::from_millis(COALESCE_MS) || buf_t.len() + buf_r.len() >= COALESCE_CHARS {
                            flush(&mut buf_t, &mut buf_r)?;
                            last = Instant::now();
                        }
                    }
                    flush(&mut buf_t, &mut buf_r)?;
                    match steered {
                        Some(s) => {
                            // Bijsturen: gedeeltelijk antwoord behouden, verder met de nieuwe aanwijzing.
                            let partial = text[start_len..].to_string();
                            let _ = log.append(Event::new(EventKind::SteerReceived, trace, serde_json::json!({"text": s, "partial_chars": partial.len()})).for_run(run_id));
                            let sep = "\n\n";
                            text.push_str(sep);
                            log.append(
                                Event::new(EventKind::MessageDelta, trace, serde_json::json!({"session_id": session, "message_id": msg, "text": sep, "reasoning": ""}))
                                    .for_run(run_id),
                            )?;
                            messages.push(Message::assistant(partial));
                            messages.push(Message::user(format!("Bijsturing van de gebruiker tijdens je antwoord: {s}\nGa verder (herhaal niet wat je al schreef), met deze bijsturing.")));
                            continue;
                        }
                        None => {
                            if let (Some(h), false) = (&hub, calls.is_empty()) {
                                // Toolronde: model vroeg tools → uitvoeren (effectanalyse: parallel waar veilig) → resultaten terug.
                                tool_rounds += 1;
                                let partial = text[start_len..].to_string();
                                let mut am = Message::assistant(partial.clone());
                                am.tool_calls = calls.clone();
                                messages.push(am);
                                let actions: Vec<Action> = calls
                                    .iter()
                                    .map(|c| {
                                        let (n, a) = h.unwrap_call(&c.name, c.arguments.clone());
                                        Action::new(format!("{run_id}:{}", c.id), n, a)
                                    })
                                    .collect();
                                let opts = ExecOptions { run: Some(run_id), trace, cancel: ctx.token().child_token(), ..Default::default() };
                                let results = h.executor.execute_plan(actions.clone(), &opts).await;
                                for ((c, a), r) in calls.iter().zip(actions.iter()).zip(results.iter()) {
                                    messages.push(Message::tool_result(c.id.clone(), c.name.clone(), r.for_model()));
                                    if r.ok {
                                        tool_steps.push(Step { tool: r.tool.clone(), input: a.input.clone() });
                                        if r.tool == "load_skill"
                                            && let Some(n) = a.input.get("name").and_then(|n| n.as_str())
                                        {
                                            used_skills.push(n.to_string());
                                        }
                                    }
                                    ws.add_note(&format!("tool {}: {}", r.tool, if r.ok { "ok" } else { "fout" }));
                                }
                                let _ = ctx.checkpoint(&serde_json::json!({"tool_round": tool_rounds, "tools": calls.iter().map(|c| c.name.clone()).collect::<Vec<_>>()}));
                                if tool_rounds >= MAX_TOOL_ROUNDS {
                                    messages.push(Message::user("Maximum aantal toolrondes bereikt. Geef nu je eindantwoord met wat je hebt."));
                                }
                                if !partial.trim().is_empty() {
                                    let sep = "\n\n";
                                    text.push_str(sep);
                                    log.append(
                                        Event::new(EventKind::MessageDelta, trace, serde_json::json!({"session_id": session, "message_id": msg, "text": sep, "reasoning": ""}))
                                            .for_run(run_id),
                                    )?;
                                }
                                continue;
                            }
                            metrics = got_metrics;
                            break;
                        }
                    }
                }
            }
            if team.is_some() {
                vortex_agents::Reporter::new(Some(log.clone()), Some(run_id), trace).done("Synthesizer", "antwoord geschreven", serde_json::json!({"chars": text.len()}));
            }
            cyc.finish(serde_json::json!({"chars": text.len()}));

            // 6. Verifiëren (licht; de Judge volgt in W09)
            cyc.enter(Phase::Verify);
            let mut issues = Vec::new();
            if text.trim().is_empty() {
                issues.push("leeg antwoord");
            }
            if mostly_english(&question, &text) {
                issues.push("antwoord niet in de taal van de vraag");
            }
            if strategy.prediction == Prediction::Abstain && !admits_unknown(&text) {
                issues.push("geen geheugen maar wel een stellig antwoord (mogelijk verzonnen)");
                ws.add_uncertainty("antwoord zonder onderbouwing uit geheugen");
            }
            verified_ok = issues.is_empty();
            let _ = log.append(Event::new(EventKind::VerificationDone, trace, serde_json::json!({"ok": issues.is_empty(), "issues": issues})).for_run(run_id));
            cyc.finish(serde_json::json!({"issues": issues.len()}));

            // 7. Reflecteren: kennisgaten en betwiste feiten → leeragenda.
            cyc.enter(Phase::Reflect);
            let topic: String = question.chars().take(120).collect();
            let factual = matches!(route.task, vortex_cognition::TaskType::Factual | vortex_cognition::TaskType::Research | vortex_cognition::TaskType::Analysis);
            if factual && (admits_unknown(&text) || best.is_none_or(|b| b < 0.2)) {
                let _ = self.agenda.add(&topic, "gap", "geen (zekere) kennis bij deze vraag", 0.5);
            }
            if conflicts > 0 {
                let _ = self.agenda.add(&topic, "disputed", "tegenstrijdige herinneringen", 0.6);
            }
            cyc.finish(serde_json::json!({}));
            Ok(metrics)
        }
        .await;

        self.steer.lock().remove(&run_id);
        if let Some(w) = self.workspaces.lock().remove(&run_id) {
            let _ = log.append(
                Event::new(
                    EventKind::WorkspaceUpdated,
                    trace,
                    serde_json::json!({"what": "final", "board": w.snapshot()}),
                )
                .for_run(run_id),
            );
        }
        let cancelled = ctx.is_cancelled()
            || matches!(
                &outcome,
                Err(ChatError::Model(ModelError::Cancelled)) | Err(ChatError::Core(CoreError::Cancelled))
            );
        let (status, err) = match &outcome {
            Ok(_) => ("complete", None),
            Err(_) if cancelled => ("cancelled", None),
            Err(e) => ("failed", Some(e.to_string())),
        };
        let metrics = outcome.as_ref().ok().cloned().flatten();
        let fin = log.transact(|tx, out| {
            SessionStore::finish_tx(tx, msg, &text, &reasoning, status, metrics.as_ref())?;
            out.emit(
                tx,
                Event::new(
                    EventKind::MessageCompleted,
                    trace,
                    serde_json::json!({"session_id": session, "message_id": msg, "status": status, "error": err, "detail": metrics}),
                )
                .for_run(run_id),
            )?;
            Ok(())
        });
        if let Err(e) = fin {
            tracing::error!(run = %run_id, error = %e, "bericht afronden mislukt");
        }
        // 8. Consolideren
        cyc.enter(Phase::Consolidate);
        if status == "complete" {
            if let Some(h) = &self.tools {
                // Skill-voorstel uit deze trace + succes/falen van gebruikte skills (W08.13/14).
                let _ = vortex_tools::skills::propose::observe(
                    &log,
                    &question,
                    &tool_steps,
                    verified_ok,
                    Some(run_id.to_string()),
                );
                used_skills.sort();
                used_skills.dedup();
                for s in &used_skills {
                    h.skills.record_outcome(s, verified_ok);
                }
            }
            self.remember_exchange(session, question, text.clone());
            if let Some(mem) = self.memory.clone() {
                // Elke 8 beurten de sessiesamenvatting vernieuwen (achtergrond).
                let n = self.sessions.messages(session).map(|m| m.len()).unwrap_or(0);
                if n >= 16 && n.is_multiple_of(16) {
                    tokio::spawn(async move {
                        let _ = mem.store.summarize_session(&session.to_string(), None).await;
                    });
                }
            }
        }
        cyc.finish(serde_json::json!({"status": status}));
        let r = match status {
            "complete" => ctx.complete(serde_json::json!({"message_id": msg})).map(|_| ()),
            "cancelled" => ctx.finish_cancelled().map(|_| ()),
            _ => ctx.fail(err.as_deref().unwrap_or("onbekend")).map(|_| ()),
        };
        if let Err(e) = r {
            tracing::warn!(run = %run_id, error = %e, "run afronden");
        }
    }
}
