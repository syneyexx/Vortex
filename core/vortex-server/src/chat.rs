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
    fn as_str(self) -> &'static str {
        match self {
            Mode::Adaptive => "adaptive",
            Mode::Quick => "quick",
            Mode::Deep => "deep",
            Mode::Research => "research",
        }
    }
    /// Redeneerniveau (gpt-oss). Metacognitie (W07) neemt dit later over in Adaptive.
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
}

const DEFAULT_TITLE: &str = "Nieuwe chat";

impl ChatEngine {
    pub fn new(kernel: Kernel) -> Self {
        let sessions = SessionStore::new(kernel.events().clone());
        Self { kernel, sessions, backends: Arc::new(Backends::default()) }
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

    fn history(&self, ctx: &RunContext, session: SessionId, upto: MessageId) -> Result<Vec<Message>, ChatError> {
        let mut msgs = Vec::new();
        if let Some(sys) = ctx.config().file("prompts/system.md") {
            msgs.push(Message::system(sys.trim()));
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
            let mut req = ChatRequest::new(self.history(&ctx, session, msg)?);
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
