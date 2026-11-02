//! HTTP-API + WebSocket-eventstream voor de GUI (en andere clients).
//!
//! * `GET /api/events?from_seq=N` (WebSocket): eerst alles na `N` uit het log (hervatten na
//!   reconnect), daarna live. Elk frame = een event (JSON) met volgnummer `seq`.
//! * REST voor snapshot en opdrachten (sessies, berichten, stoppen, status, instellingen).
// VX:W03.01

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use vortex_core::{CoreError, Kernel};
use vortex_types::{Event, EventKind, MessageId, RunId, SessionId, TraceId};

use crate::chat::{ChatEngine, ChatError, SendRequest};
use crate::live::{Live, Subsystem};

#[derive(Debug, Clone)]
pub struct AppState {
    pub kernel: Kernel,
    pub chat: ChatEngine,
    pub live: Live,
    pub memory: crate::memory_api::MemoryHandle,
    /// Tools + skills (W08).
    pub tools: Option<Arc<vortex_tools::ToolHub>>,
}

#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl ApiError {
    pub fn new(code: StatusCode, msg: impl Into<String>) -> Self {
        Self(code, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        let code = match e {
            CoreError::NotFound(_) => StatusCode::NOT_FOUND,
            CoreError::InvalidTransition { .. } | CoreError::Config(_) => StatusCode::BAD_REQUEST,
            CoreError::VersionConflict { .. } => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(code, e.to_string())
    }
}

impl From<ChatError> for ApiError {
    fn from(e: ChatError) -> Self {
        match e {
            ChatError::Core(c) => c.into(),
            ChatError::Bad(m) => ApiError(StatusCode::BAD_REQUEST, m),
            ChatError::Model(m) => ApiError(StatusCode::BAD_GATEWAY, m.to_string()),
        }
    }
}

type R<T> = Result<Json<T>, ApiError>;

fn parse<T: std::str::FromStr>(s: &str) -> Result<T, ApiError> {
    s.parse().map_err(|_| ApiError(StatusCode::BAD_REQUEST, format!("ongeldig ID: {s}")))
}

pub fn router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/health", get(|| async { Json(json!({"ok": true})) }))
        .route("/api/status", get(status))
        .route("/api/events", get(events_ws))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session).patch(rename_session).delete(archive_session))
        .route("/api/sessions/:id/messages", post(send_message))
        .route("/api/sessions/:id/fork", post(fork_session))
        .route("/api/runs", get(list_runs))
        .route("/api/runs/:id/stop", post(stop_run))
        .route("/api/runs/:id/pause", post(pause_run))
        .route("/api/runs/:id/resume", post(resume_run))
        .route("/api/models", get(models))
        .route("/api/settings/prompts", get(get_prompts).put(put_prompt))
        .route("/api/settings/history", get(config_history))
        .route("/api/settings/restore", post(restore_prompt))
        .merge(crate::memory_api::routes())
        .merge(crate::eval_api::routes())
        .merge(crate::observe_api::routes())
        .merge(crate::tools_api::routes())
        .merge(crate::team_api::routes())
        .with_state(state);
    match static_dir {
        Some(d) if d.join("index.html").exists() => api.fallback_service(
            tower_http::services::ServeDir::new(&d)
                .fallback(tower_http::services::ServeFile::new(d.join("index.html"))),
        ),
        _ => api,
    }
    .layer(tower_http::cors::CorsLayer::permissive())
}

// ---------- status ----------

async fn status(State(s): State<AppState>) -> R<Value> {
    let live = s.live.snapshot();
    let reg = s.chat.registry().ok();
    let db_ok = s.kernel.events().last_seq().is_ok();
    let main_ready = live.servers.iter().find(|x| x.name == "main").is_some_and(|x| x.ready);
    let aux_ready = live.servers.iter().find(|x| x.name == "aux").is_some_and(|x| x.ready);
    let subsystems = vec![
        Subsystem { name: "kern".into(), ok: true, detail: "draait".into() },
        Subsystem {
            name: "database".into(),
            ok: db_ok,
            detail: if db_ok { "schrijfbaar".into() } else { "fout".into() },
        },
        Subsystem {
            name: "model".into(),
            ok: main_ready,
            detail: if main_ready { "gereed".into() } else { "niet bereikbaar".into() },
        },
        Subsystem {
            name: "1060-team".into(),
            ok: aux_ready,
            detail: if aux_ready { "gereed".into() } else { "niet bereikbaar".into() },
        },
        Subsystem { name: "config".into(), ok: reg.is_some(), detail: s.kernel.config().current().version.clone() },
    ];
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "subsystems": subsystems,
        "gpus": live.gpus,
        "gpu_detail": live.gpu_detail,
        "servers": live.servers,
        "default_model": reg.as_ref().map(|r| r.default.clone()),
        "tok_s": live.tok_s,
        "last_seq": s.kernel.events().last_seq()?,
        "active_runs": s.kernel.runs().active()?.len(),
        "dirigent": s.kernel.dirigent().snapshot(),
    })))
}

// ---------- eventstream ----------

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    from_seq: i64,
}

async fn events_ws(State(s): State<AppState>, Query(q): Query<EventsQuery>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| stream_events(s.kernel, q.from_seq, socket))
}

/// Replay vanaf `from_seq`, daarna live. Volgnummers zijn strikt oplopend zonder gaten.
async fn stream_events(kernel: Kernel, from_seq: i64, mut socket: WebSocket) {
    // Eerst abonneren, dan inhalen: zo valt er niets tussen wal en schip.
    let mut rx = kernel.events().subscribe();
    let mut last = from_seq;
    let send = |e: &Event| WsMessage::Text(serde_json::to_string(e).unwrap_or_default());
    loop {
        let batch = match kernel.events().read_from(last, 500) {
            Ok(b) => b,
            Err(_) => return,
        };
        if batch.is_empty() {
            break;
        }
        for e in &batch {
            if socket.send(send(e)).await.is_err() {
                return; // verbinding weg ≠ taak gestopt: runs lopen gewoon door
            }
            last = e.seq;
        }
    }
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(e) => {
                    if e.seq <= last { continue; }
                    if e.seq > last + 1 {
                        // gat (bv. door gelijktijdige commits): bijlezen uit het log
                        if let Ok(b) = kernel.events().read_from(last, 500) {
                            for x in b.iter().filter(|x| x.seq < e.seq) {
                                if socket.send(send(x)).await.is_err() { return; }
                            }
                        }
                    }
                    if socket.send(send(&e)).await.is_err() { return; }
                    last = e.seq;
                }
                Err(RecvError::Lagged(_)) => {
                    // achtergeraakt: inhalen uit het log
                    while let Ok(b) = kernel.events().read_from(last, 500) {
                        if b.is_empty() { break; }
                        for x in &b {
                            if socket.send(send(x)).await.is_err() { return; }
                            last = x.seq;
                        }
                    }
                }
                Err(RecvError::Closed) => return,
            },
            msg = socket.recv() => match msg {
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
        }
    }
}

// ---------- sessies ----------

#[derive(Debug, Deserialize)]
struct ListQ {
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    50
}

async fn list_sessions(State(s): State<AppState>, Query(q): Query<ListQ>) -> R<Value> {
    Ok(Json(json!(s.chat.sessions().list(q.limit)?)))
}

async fn create_session(State(s): State<AppState>) -> R<Value> {
    Ok(Json(json!(s.chat.new_session()?)))
}

async fn get_session(State(s): State<AppState>, AxPath(id): AxPath<String>) -> R<Value> {
    let id: SessionId = parse(&id)?;
    Ok(Json(json!({"session": s.chat.sessions().get(id)?, "messages": s.chat.sessions().messages(id)?})))
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    title: String,
}

async fn rename_session(State(s): State<AppState>, AxPath(id): AxPath<String>, Json(b): Json<RenameBody>) -> R<Value> {
    s.chat.sessions().rename(parse(&id)?, b.title.trim())?;
    Ok(Json(json!({"ok": true})))
}

async fn archive_session(State(s): State<AppState>, AxPath(id): AxPath<String>) -> R<Value> {
    s.chat.sessions().archive(parse(&id)?)?;
    Ok(Json(json!({"ok": true})))
}

async fn send_message(State(s): State<AppState>, AxPath(id): AxPath<String>, Json(b): Json<SendRequest>) -> R<Value> {
    Ok(Json(json!(s.chat.send(parse(&id)?, b)?)))
}

#[derive(Debug, Deserialize)]
struct ForkBody {
    message_id: String,
}

async fn fork_session(State(s): State<AppState>, AxPath(id): AxPath<String>, Json(b): Json<ForkBody>) -> R<Value> {
    let at: MessageId = parse(&b.message_id)?;
    Ok(Json(json!(s.chat.sessions().fork(parse(&id)?, at)?)))
}

// ---------- runs ----------

async fn list_runs(State(s): State<AppState>) -> R<Value> {
    Ok(Json(json!({"active": s.kernel.runs().active()?, "recent": s.kernel.runs().recent(30)?})))
}

async fn stop_run(State(s): State<AppState>, AxPath(id): AxPath<String>) -> R<Value> {
    let id: RunId = parse(&id)?;
    Ok(Json(json!({"accepted": s.kernel.request_stop(id)?})))
}

async fn pause_run(State(s): State<AppState>, AxPath(id): AxPath<String>) -> R<Value> {
    s.kernel.dirigent().hold(parse(&id)?, true);
    Ok(Json(json!({"paused": true})))
}

async fn resume_run(State(s): State<AppState>, AxPath(id): AxPath<String>) -> R<Value> {
    s.kernel.dirigent().hold(parse(&id)?, false);
    Ok(Json(json!({"paused": false})))
}

// ---------- modellen ----------

async fn models(State(s): State<AppState>) -> R<Value> {
    let reg = s.chat.registry()?;
    let live = s.live.snapshot();
    let models: Vec<Value> = reg
        .models
        .iter()
        .map(|(n, m)| {
            let srv = live.servers.iter().find(|x| x.name == m.gpu);
            json!({
                "name": n, "roles": m.roles, "server": m.gpu, "file": m.file, "context": m.context,
                "kv_type": m.kv_type, "parser": m.toolcall_parser,
                "loaded": srv.is_some_and(|x| x.ready && x.model.as_deref() == Some(n.as_str())),
                "chat": m.roles.iter().any(|r| matches!(r.as_str(), "chat" | "code" | "deep" | "agents")),
            })
        })
        .collect();
    Ok(Json(json!({"default": reg.default, "models": models, "servers": live.servers})))
}

// ---------- instellingen (prompts) ----------
// VX:W03.12

/// Alleen `.md` onder `prompts/` of `roles/`, zonder `..` (padveiligheid = techniek, geen policy).
fn safe_rel(rel: &str) -> Result<PathBuf, ApiError> {
    let p = Path::new(rel);
    let ok = p.components().all(|c| matches!(c, Component::Normal(_)))
        && (rel.starts_with("prompts/") || rel.starts_with("roles/"))
        && rel.ends_with(".md");
    if !ok {
        return Err(ApiError(StatusCode::BAD_REQUEST, format!("ongeldig pad: {rel}")));
    }
    Ok(p.to_path_buf())
}

async fn get_prompts(State(s): State<AppState>) -> R<Value> {
    let cfg = s.kernel.config().current();
    let files: Vec<Value> = cfg
        .files
        .iter()
        .filter(|(k, _)| (k.starts_with("prompts/") || k.starts_with("roles/")) && k.ends_with(".md"))
        .map(|(k, v)| json!({"path": k, "content": v}))
        .collect();
    Ok(Json(json!({"version": cfg.version, "files": files})))
}

#[derive(Debug, Deserialize)]
struct PutPrompt {
    path: String,
    content: String,
}

async fn put_prompt(State(s): State<AppState>, Json(b): Json<PutPrompt>) -> R<Value> {
    let rel = safe_rel(&b.path)?;
    let full = s.kernel.config().dir().join(&rel);
    if let Some(d) = full.parent() {
        std::fs::create_dir_all(d).map_err(CoreError::from)?;
    }
    std::fs::write(&full, &b.content).map_err(CoreError::from)?;
    s.kernel.config().reload()?;
    let v = s.kernel.config().current().version.clone();
    s.kernel.events().append(Event::new(
        EventKind::PromptUpdated,
        TraceId::new(),
        json!({"path": b.path, "version": v}),
    ))?;
    // Live vanaf het volgende bericht: nieuwe runs pakken de nieuwe versie; lopende runs houden hun eigen.
    Ok(Json(json!({"version": v})))
}

async fn config_history(State(s): State<AppState>) -> R<Value> {
    let h = s.kernel.config().history()?;
    Ok(Json(json!(h.into_iter().map(|(v, ts)| json!({"version": v, "ts": ts})).collect::<Vec<_>>())))
}

#[derive(Debug, Deserialize)]
struct RestoreBody {
    version: String,
    path: String,
}

async fn restore_prompt(State(s): State<AppState>, Json(b): Json<RestoreBody>) -> R<Value> {
    let rel = safe_rel(&b.path)?;
    let content: Option<String> = s.kernel.db().read(|c| {
        use rusqlite::OptionalExtension;
        Ok(c.query_row("SELECT content FROM config_versions WHERE version = ?1", [&b.version], |r| {
            r.get::<_, String>(0)
        })
        .optional()?)
    })?;
    let content = content.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("versie {} onbekend", b.version)))?;
    let v: Value = serde_json::from_str(&content).map_err(CoreError::from)?;
    let text = v["files"][&b.path]
        .as_str()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("{} bestond niet in {}", b.path, b.version)))?
        .to_string();
    put_prompt(State(s), Json(PutPrompt { path: rel.to_string_lossy().replace('\\', "/"), content: text })).await
}

/// Gedeelde staat opbouwen + achtergrondtaken starten.
pub fn build_state(kernel: Kernel) -> anyhow::Result<AppState> {
    let memory = crate::memory_api::build(&kernel)?;
    crate::memory_api::spawn_maintenance(memory.clone(), kernel.shutdown_token().child_token());
    let mut chat = ChatEngine::new(kernel.clone(), Some(memory.clone()));
    let tools = crate::tools_api::build(&kernel, Some(memory.clone()));
    chat.tools = Some(tools.clone());
    chat.online = Arc::new(vortex_agents::connectivity::Connectivity::new(tools.cfg.searxng_url.clone()));
    if let Err(e) = vortex_agents::roles::ensure_defaults(kernel.config().dir()) {
        tracing::warn!(error = %e, "standaardrollen schrijven");
    }
    crate::team_api::spawn_conflict_worker(chat.clone(), kernel.shutdown_token().child_token());
    if let Ok(n) = chat.sessions().repair_streaming(kernel.runs())
        && n > 0
    {
        tracing::warn!(hersteld = n, "berichten die tijdens een crash streamden zijn hersteld");
    }
    let live = Live::default();
    live.spawn(kernel.clone(), kernel.shutdown_token().child_token());
    // Drukmeters + idle-detectie voeden de Dirigent (W04).
    vortex_core::sampler::spawn(
        kernel.dirigent().clone(),
        vortex_core::sampler::SamplerConfig::default(),
        kernel.shutdown_token().child_token(),
    );
    Ok(AppState { kernel, chat, live, memory, tools: Some(tools) })
}

pub type Shared = Arc<AppState>;
