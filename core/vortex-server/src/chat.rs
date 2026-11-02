//! Chat-engine: bericht → run (P0) → GPU-slot → model streamt → gecoalesceerde deltas in het
//! event log → bericht afgerond. De run leeft los van de GUI-verbinding (verbinding weg ≠ gestopt).
// VX:W03.02

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vortex_core::{CoreError, Kernel, RunContext, RunSpec};
use vortex_models::backend::{Backend, ChatRequest, Message};
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
    pub fn reasoning_effort(self) -> &'static str {
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
    pub(crate) kernel: Kernel,
    pub(crate) sessions: SessionStore,
    pub(crate) backends: Arc<Backends>,
    pub(crate) memory: Option<crate::memory_api::MemoryHandle>,
    /// Bijsturing tijdens een lopende run (W07.10).
    pub(crate) steer: Arc<Mutex<HashMap<RunId, tokio::sync::mpsc::Sender<String>>>>,
    /// Global Workspace per lopende run.
    pub(crate) workspaces: Arc<Mutex<HashMap<RunId, vortex_cognition::Workspace>>>,
    pub(crate) agenda: vortex_cognition::Agenda,
    /// Tools + skills (W08); `None` = chat zonder tools.
    pub tools: Option<Arc<vortex_tools::ToolHub>>,
    /// Embeddings van skillbeschrijvingen (cache voor skillselectie).
    pub(crate) skill_vecs: Arc<Mutex<HashMap<String, Vec<f32>>>>,
}

/// Maximale tijd voor geheugen-ophalen vóór een antwoord (chat wacht nooit langer).
pub const MEMORY_TIMEOUT_MS: u64 = 1500;

const DEFAULT_TITLE: &str = "Nieuwe chat";

impl ChatEngine {
    pub fn new(kernel: Kernel, memory: Option<crate::memory_api::MemoryHandle>) -> Self {
        let sessions = SessionStore::new(kernel.events().clone());
        let agenda = vortex_cognition::Agenda::new(kernel.events().clone());
        Self {
            kernel,
            sessions,
            backends: Arc::new(Backends::default()),
            memory,
            steer: Arc::new(Mutex::new(HashMap::new())),
            workspaces: Arc::new(Mutex::new(HashMap::new())),
            agenda,
            tools: None,
            skill_vecs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Herinneringen ophalen voor deze vraag → tekstblok voor de system prompt.
    // VX:W05.06
    pub(crate) async fn memory_context(
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
    pub(crate) fn log_context(
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
    pub(crate) fn remember_exchange(&self, session: SessionId, question: String, answer: String) {
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
}
