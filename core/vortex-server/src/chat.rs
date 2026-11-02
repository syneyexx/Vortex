//! Chat-engine: bericht → run (P0) → GPU-slot → model streamt → gecoalesceerde deltas in het
//! event log → bericht afgerond. De run leeft los van de GUI-verbinding (verbinding weg ≠ gestopt).
// VX:W03.02

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vortex_core::{CoreError, Kernel, RunContext, RunSpec};
use vortex_models::backend::{Backend, ChatRequest, Message, StreamEvent};
use vortex_models::{ModelError, OpenAiBackend, Registry};
use vortex_types::{Event, EventKind, MessageId, Priority, RunId, RunKind, SessionId};

use crate::sessions::{NewMessage, SessionStore};

/// Antwoordmodus uit de invoerbalk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Adaptive,
    Quick,
    Deep,
    Research,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Adaptive => "adaptive",
            Mode::Quick => "quick",
            Mode::Deep => "deep",
            Mode::Research => "research",
        }
    }
    /// Redeneerniveau (gpt-oss). Metacognitie (W07) neemt dit later over in Adaptive.
    pub fn parse(s: &str) -> Self {
        match s {
            "quick" => Mode::Quick,
            "deep" => Mode::Deep,
            "research" => Mode::Research,
            _ => Mode::Adaptive,
        }
    }
    fn reasoning_effort(self) -> &'static str {
        match self {
            Mode::Quick => "low",
            Mode::Adaptive => "medium",
            Mode::Deep | Mode::Research => "high",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendRequest {
    pub text: String,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendReply {
    pub run_id: RunId,
    pub user_message_id: MessageId,
    pub assistant_message_id: MessageId,
    pub model: String,
}

/// Tijdsvenster/grootte voor het samenvoegen van deltas (UI-event coalescing).
// VX:W03.03
pub const COALESCE_MS: u64 = 40;
pub const COALESCE_CHARS: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("ongeldig verzoek: {0}")]
    Bad(String),
}

/// Levert backends per model (cache). Model wisselen gaat in W02 via `ModelServer`.
#[derive(Debug, Default)]
pub struct Backends {
    cache: Mutex<HashMap<(String, String), Arc<OpenAiBackend>>>,
}

impl Backends {
    pub fn for_model(&self, reg: &Registry, model: &str) -> Result<Arc<OpenAiBackend>, ChatError> {
        let entry = reg.models.get(model).ok_or_else(|| ChatError::Bad(format!("onbekend model '{model}'")))?;
        let server =
            reg.server(&entry.gpu).ok_or_else(|| ChatError::Bad(format!("server '{}' onbekend", entry.gpu)))?;
        let key = (server.url.clone(), model.to_string());
        if let Some(b) = self.cache.lock().get(&key) {
            return Ok(b.clone());
        }
        let caps = entry.capabilities.clone().unwrap_or_else(|| {
            let mut c = vortex_models::openai::llama_server_caps();
            c.reasoning_effort = entry.file.to_lowercase().contains("gpt-oss");
            c.vision = entry.roles.iter().any(|r| r == "vision");
            c
        });
        let b = Arc::new(OpenAiBackend::new(&entry.gpu, &server.url, model, &caps, &entry.toolcall_parser)?);
        self.cache.lock().insert(key, b.clone());
        Ok(b)
    }
}

#[derive(Debug, Clone)]
pub struct ChatEngine {
    kernel: Kernel,
    sessions: SessionStore,
    backends: Arc<Backends>,
    memory: Option<crate::memory_api::MemoryHandle>,
}

/// Maximale tijd voor geheugen-ophalen vóór een antwoord (chat wacht nooit langer).
pub const MEMORY_TIMEOUT_MS: u64 = 1500;

const DEFAULT_TITLE: &str = "Nieuwe chat";

impl ChatEngine {
    pub fn new(kernel: Kernel, memory: Option<crate::memory_api::MemoryHandle>) -> Self {
        let sessions = SessionStore::new(kernel.events().clone());
        Self { kernel, sessions, backends: Arc::new(Backends::default()), memory }
    }

    /// Herinneringen ophalen voor deze vraag → tekstblok voor de system prompt.
    // VX:W05.06
    async fn memory_context(
        &self,
        run: RunId,
        question: &str,
        workspace: Option<String>,
    ) -> Option<(String, Vec<serde_json::Value>)> {
        let mem = self.memory.as_ref()?;
        let mut q = vortex_memory::Query::new(question);
        q.k = 6;
        q.run_id = Some(run);
        q.filters.workspace = workspace;
        let r = tokio::time::timeout(
            Duration::from_millis(MEMORY_TIMEOUT_MS),
            mem.store.retrieve(&q, mem.reranker.as_ref()),
        )
        .await;
        let items = match r {
            Ok(Ok(items)) if !items.is_empty() => items,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "geheugen ophalen mislukt");
                return None;
            }
            _ => return None,
        };
        let mut out = String::from("Relevante herinneringen (eigen geheugen; noem ze als je ze gebruikt):\n");
        let used: Vec<serde_json::Value> = items
            .iter()
            .map(|it| serde_json::json!({"id": it.memory.id, "kind": it.memory.kind, "score": it.score.final_score, "counter": it.counter_evidence}))
            .collect();
        for it in items {
            let m = &it.memory;
            let src = m.source_uri.as_deref().unwrap_or(m.provenance.origin.as_deref().unwrap_or("geheugen"));
            let date = m.event_time.as_deref().unwrap_or(&m.created_at).get(..10).unwrap_or("");
            let text: String = m.content.chars().take(600).collect();
            if it.counter_evidence {
                out.push_str(&format!(
                    "- LET OP, mogelijk tegenstrijdig [{}] ({src}, {date}): {text}\n",
                    m.kind.as_str()
                ));
            } else {
                out.push_str(&format!("- [{}] ({src}, {date}): {text}\n", m.kind.as_str()));
            }
        }
        Some((out, used))
    }

    /// Context Assembly Inspector: wat er in de prompt ging (en wat wegviel). VX:W06.06
    fn log_context(
        &self,
        run: RunId,
        trace: vortex_types::TraceId,
        ctx: &RunContext,
        msgs: &[Message],
        memory: &[serde_json::Value],
        dropped: usize,
    ) {
        let chars: usize = msgs.iter().map(|m| m.content.len()).sum();
        let _ = self.kernel.events().append(
            Event::new(
                EventKind::ContextAssembled,
                trace,
                serde_json::json!({
                    "config_version": ctx.config().version,
                    "system_prompt_chars": msgs.first().filter(|m| m.role == vortex_models::Role::System).map(|m| m.content.len()),
                    "messages": msgs.len(),
                    "history_messages": msgs.iter().filter(|m| m.role != vortex_models::Role::System).count(),
                    "memories": memory,
                    "approx_tokens": chars / 4,
                    "dropped_messages": dropped,
                }),
            )
            .for_run(run),
        );
    }

    /// Eén antwoord zonder sessie of opslag (eval). Loopt via de Dirigent als P2-run.
    // VX:W06.01
    pub async fn answer_once(&self, prompt: &str, mode: Mode, workspace: Option<String>) -> Result<String, ChatError> {
        let reg = self.registry()?;
        let backend = self.backends.for_model(&reg, &reg.default)?;
        let mut spec = RunSpec::new(RunKind::Eval, Priority::P2);
        spec.input = serde_json::json!({"mode": mode.as_str(), "eval": true});
        let ctx = self.kernel.start_run(spec)?;
        ctx.start()?;
        let run = ctx.id();
        let trace = ctx.trace().trace_id;
        let r: Result<String, ChatError> = async {
            let _gpu = ctx.gpu(Priority::P2).await?;
            let mem = self.memory_context(run, prompt, workspace).await;
            let mut sys = ctx.config().file("prompts/system.md").map(|s| s.trim().to_string()).unwrap_or_default();
            let used = mem.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
            if let Some((m, _)) = mem {
                sys.push_str("\n\n");
                sys.push_str(&m);
            }
            let msgs = vec![Message::system(sys), Message::user(prompt)];
            self.log_context(run, trace, &ctx, &msgs, &used, 0);
            let mut req = ChatRequest::new(msgs);
            if backend.capabilities().reasoning_effort {
                req.reasoning_effort = Some(mode.reasoning_effort().into());
            }
            let resp = backend.chat(&req, ctx.token()).await?;
            let _ = self.kernel.events().append(resp.metrics.to_event(trace, Some(run)));
            Ok(resp.content)
        }
        .await;
        match &r {
            Ok(t) => {
                let _ = ctx.complete(serde_json::json!({"chars": t.len()}));
            }
            Err(e) => {
                let _ = ctx.fail(&e.to_string());
            }
        }
        r
    }

    /// Na een afgerond antwoord: episodisch geheugen + persoonlijke feiten (via de policy).
    fn remember_exchange(&self, session: SessionId, question: String, answer: String) {
        let Some(mem) = self.memory.clone() else { return };
        tokio::spawn(async move {
            use vortex_memory::{MemoryKind, NewMemory};
            use vortex_types::{Provenance, SourceKind};
            let uri = format!("chat:{session}");
            let mut ep = NewMemory::new(
                MemoryKind::Episodic,
                format!("Gebruiker: {question}\nVortex: {}", answer.chars().take(2000).collect::<String>()),
                Provenance::with_origin(SourceKind::User, uri.clone()),
            );
            ep.session_id = Some(session);
            ep.source_uri = Some(uri.clone());
            ep.source_hash = Some("chat".into());
            ep.title = question.chars().take(80).collect();
            // Belang komt uit wat de gebruiker zei, niet uit het antwoord van het model.
            ep.importance = Some(vortex_memory::policy::importance(&question, MemoryKind::Episodic).0);
            if let Err(e) = mem.store.write_or_defer(ep).await {
                tracing::warn!(error = %e, "episodisch geheugen schrijven mislukt");
            }
            let f = vortex_memory::policy::features(&question);
            if f.personal || f.explicit_remember {
                let mut u =
                    NewMemory::new(MemoryKind::User, question, Provenance::with_origin(SourceKind::User, uri.clone()));
                u.session_id = Some(session);
                u.source_uri = Some(uri);
                u.source_hash = Some("chat".into());
                if let Err(e) = mem.store.write_or_defer(u).await {
                    tracing::warn!(error = %e, "gebruikersfeit schrijven mislukt");
                }
            }
        });
    }

    pub fn sessions(&self) -> &SessionStore {
        &self.sessions
    }

    pub fn registry(&self) -> Result<Registry, ChatError> {
        Ok(Registry::from_config(&self.kernel.config().current())?)
    }

    pub fn new_session(&self) -> Result<crate::sessions::Session, ChatError> {
        Ok(self.sessions.create(DEFAULT_TITLE)?)
    }

    /// Stuur een bericht. Keert direct terug; het antwoord komt via de eventstream.
    pub fn send(&self, session: SessionId, req: SendRequest) -> Result<SendReply, ChatError> {
        let text = req.text.trim().to_string();
        if text.is_empty() {
            return Err(ChatError::Bad("leeg bericht".into()));
        }
        let sess = self.sessions.get(session)?;
        let reg = self.registry()?;
        let model = req.model.clone().unwrap_or_else(|| reg.default.clone());
        let backend = self.backends.for_model(&reg, &model)?;
        let mut spec = RunSpec::new(RunKind::Chat, Priority::P0);
        spec.input = serde_json::json!({"session_id": session, "mode": req.mode.as_str(), "model": model});
        let ctx = self.kernel.start_run(spec)?;
        let run_id = ctx.id();
        let trace = ctx.trace().trace_id;
        let cfg_version = ctx.config().version.clone();
        let (user_id, asst_id) = self.kernel.events().transact(|tx, out| {
            let u = SessionStore::insert_tx(
                tx,
                session,
                &NewMessage { role: "user".into(), content: text.clone(), status: "complete".into(), run_id: Some(run_id), ..Default::default() },
            )?;
            let a = SessionStore::insert_tx(
                tx,
                session,
                &NewMessage {
                    role: "assistant".into(),
                    status: "streaming".into(),
                    run_id: Some(run_id),
                    model: Some(model.clone()),
                    mode: Some(req.mode.as_str().into()),
                    config_version: Some(cfg_version.clone()),
                    ..Default::default()
                },
            )?;
            out.emit(
                tx,
                Event::new(
                    EventKind::MessageReceived,
                    trace,
                    serde_json::json!({"session_id": session, "message_id": u, "text": text, "mode": req.mode.as_str()}),
                )
                .for_run(run_id),
            )?;
            out.emit(
                tx,
                Event::new(
                    EventKind::MessageStarted,
                    trace,
                    serde_json::json!({"session_id": session, "message_id": a, "model": model, "config_version": cfg_version}),
                )
                .for_run(run_id),
            )?;
            Ok((u, a))
        })?;
        if sess.title == DEFAULT_TITLE {
            let title: String = text.chars().take(60).collect();
            let _ = self.sessions.rename(session, &title);
        }
        let me = self.clone();
        let mode = req.mode;
        tokio::spawn(async move {
            me.run(ctx, backend, session, asst_id, mode).await;
        });
        Ok(SendReply { run_id, user_message_id: user_id, assistant_message_id: asst_id, model })
    }

    fn history(
        &self,
        ctx: &RunContext,
        session: SessionId,
        upto: MessageId,
        memory: Option<String>,
    ) -> Result<Vec<Message>, ChatError> {
        let mut msgs = Vec::new();
        let mut sys = ctx.config().file("prompts/system.md").map(|s| s.trim().to_string()).unwrap_or_default();
        if let Some(m) = memory {
            if !sys.is_empty() {
                sys.push_str("\n\n");
            }
            sys.push_str(&m);
        }
        if !sys.is_empty() {
            msgs.push(Message::system(sys));
        }
        for m in self.sessions.messages(session)? {
            if m.id == upto {
                break;
            }
            if m.content.is_empty() {
                continue;
            }
            match m.role.as_str() {
                "user" => msgs.push(Message::user(m.content)),
                "assistant" if m.status == "complete" => msgs.push(Message::assistant(m.content)),
                _ => {}
            }
        }
        Ok(msgs)
    }

    async fn run(&self, ctx: RunContext, backend: Arc<OpenAiBackend>, session: SessionId, msg: MessageId, mode: Mode) {
        let run_id = ctx.id();
        let trace = ctx.trace().trace_id;
        let log = self.kernel.events().clone();
        let mut text = String::new();
        let mut reasoning = String::new();
        let outcome: Result<Option<serde_json::Value>, ChatError> = async {
            ctx.start()?;
            let _gpu = ctx.gpu(Priority::P0).await?;
            let question = self
                .sessions
                .messages(session)?
                .into_iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content)
                .unwrap_or_default();
            let mem_ctx = self.memory_context(run_id, &question, None).await;
            let used = mem_ctx.as_ref().map(|(_, u)| u.clone()).unwrap_or_default();
            let msgs = self.history(&ctx, session, msg, mem_ctx.map(|(t, _)| t))?;
            self.log_context(run_id, trace, &ctx, &msgs, &used, 0);
            let mut req = ChatRequest::new(msgs);
            if backend.capabilities().reasoning_effort {
                req.reasoning_effort = Some(mode.reasoning_effort().into());
            }
            let mut stream = backend.chat_stream(&req, ctx.token().child_token()).await?;
            let (mut buf_t, mut buf_r) = (String::new(), String::new());
            let mut last = Instant::now();
            let mut metrics = None;
            let flush = |t: &mut String, r: &mut String| -> Result<(), ChatError> {
                if t.is_empty() && r.is_empty() {
                    return Ok(());
                }
                log.append(
                    Event::new(
                        EventKind::MessageDelta,
                        trace,
                        serde_json::json!({"session_id": session, "message_id": msg, "text": t, "reasoning": r}),
                    )
                    .for_run(run_id),
                )?;
                t.clear();
                r.clear();
                Ok(())
            };
            loop {
                let next = tokio::time::timeout(Duration::from_millis(COALESCE_MS), stream.next()).await;
                match next {
                    Err(_) => {} // tijdvenster verstreken: flushen
                    Ok(None) => break,
                    Ok(Some(ev)) => match ev? {
                        StreamEvent::Text(t) => {
                            text.push_str(&t);
                            buf_t.push_str(&t);
                        }
                        StreamEvent::Reasoning(r) => {
                            reasoning.push_str(&r);
                            buf_r.push_str(&r);
                        }
                        StreamEvent::Done { metrics: m, usage, finish_reason } => {
                            let _ = ctx.charge_tokens(usage.completion_tokens);
                            log.append(m.to_event(trace, Some(run_id)))?;
                            metrics = Some(serde_json::json!({
                                "metrics": m, "usage": usage, "finish_reason": finish_reason
                            }));
                        }
                        StreamEvent::ToolCallStreaming { .. } | StreamEvent::ToolCallReady(_) => {}
                    },
                }
                if last.elapsed() >= Duration::from_millis(COALESCE_MS) || buf_t.len() + buf_r.len() >= COALESCE_CHARS {
                    flush(&mut buf_t, &mut buf_r)?;
                    last = Instant::now();
                }
            }
            flush(&mut buf_t, &mut buf_r)?;
            Ok(metrics)
        }
        .await;

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
        if status == "complete" {
            let q = self
                .sessions
                .messages(session)
                .ok()
                .and_then(|ms| ms.into_iter().rev().find(|m| m.role == "user").map(|m| m.content))
                .unwrap_or_default();
            self.remember_exchange(session, q, text.clone());
        }
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
