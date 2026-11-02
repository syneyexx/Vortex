//! Geheugen in de server: opbouw uit de registry + HTTP-endpoints voor de Memory-pagina.
// VX:W05.13

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as AxPath, Query as AxQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_context::adapters::{ModelEmbedder, ModelMemLlm, ModelReranker};
use vortex_core::Kernel;
use vortex_memory::{
    BatchingEmbedder, Embedder, Filters, HashEmbedder, HeuristicLlm, MemConfig, MemLlm, MemoryKind, MemoryStore,
    NewMemory, Query, Reranker,
};
use vortex_models::{OpenAiBackend, Registry};
use vortex_types::{MemoryId, Provenance, SourceKind};

use crate::api::{ApiError, AppState};

/// Geheugen + optionele reranker.
#[derive(Debug, Clone)]
pub struct MemoryHandle {
    pub store: Arc<MemoryStore>,
    pub reranker: Option<Arc<dyn Reranker>>,
}

fn backend_for(reg: &Registry, role: &str) -> Option<(String, Arc<OpenAiBackend>, Option<u32>)> {
    let (name, entry) = reg.for_role(role)?;
    let srv = reg.server(&entry.gpu)?;
    let mut caps = vortex_models::openai::llama_server_caps();
    caps.reasoning_effort = false;
    let b = OpenAiBackend::new(&entry.gpu, &srv.url, name, &caps, &entry.toolcall_parser).ok()?;
    Some((name.to_string(), Arc::new(b), entry.dims))
}

/// Bouw het geheugen uit de config. Geen embeddingmodel geconfigureerd → hash-embedder (gemeld).
pub fn build(kernel: &Kernel) -> anyhow::Result<MemoryHandle> {
    let reg = Registry::from_config(&kernel.config().current()).ok();
    let (embedder, dims): (Arc<dyn Embedder>, usize) = match reg.as_ref().and_then(|r| backend_for(r, "embedding")) {
        Some((name, b, dims)) => {
            let dims = dims.unwrap_or(512) as usize;
            let inner: Arc<dyn Embedder> = Arc::new(ModelEmbedder::new(b, &name, 4096));
            (Arc::new(BatchingEmbedder::new(inner, dims, 32, Duration::from_millis(5))), dims)
        }
        None => {
            tracing::warn!("geen embeddingmodel in config/models.toml (rol 'embedding'); hash-embedder als terugval");
            (Arc::new(HashEmbedder::new(512)), 512)
        }
    };
    let llm: Arc<dyn MemLlm> = match reg.as_ref().and_then(|r| backend_for(r, "router")) {
        Some((_, b, _)) => Arc::new(ModelMemLlm::new(b)),
        None => Arc::new(HeuristicLlm),
    };
    let reranker: Option<Arc<dyn Reranker>> = reg
        .as_ref()
        .and_then(|r| backend_for(r, "rerank"))
        .map(|(_, b, _)| Arc::new(ModelReranker(b)) as Arc<dyn Reranker>);
    let store = MemoryStore::open(kernel.events().clone(), embedder, llm, MemConfig { dims, ..Default::default() })?;
    Ok(MemoryHandle { store: Arc::new(store), reranker })
}

/// Achtergrond: wachtrij opnieuw proberen (30 s) en decay (elk uur).
pub fn spawn_maintenance(mem: MemoryHandle, stop: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
            if let Ok((ok, left)) = mem.store.retry_pending(64).await
                && ok > 0
            {
                tracing::info!(ok, left, "uitgestelde herinneringen verwerkt");
            }
            tick += 1;
            if tick.is_multiple_of(120) {
                let _ = mem.store.decay(24.0, 0.5);
            }
        }
    });
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/memory", get(list).post(create))
        .route("/api/memory/stats", get(stats))
        .route("/api/memory/search", post(search))
        .route("/api/memory/conflicts", get(conflicts))
        .route("/api/memory/conflicts/:id/resolve", post(resolve))
        .route("/api/memory/workspaces", get(workspaces))
        .route("/api/memory/export", get(export))
        .route("/api/memory/import", post(import))
        .route("/api/memory/backup", post(backup))
        .route("/api/memory/:id", get(one).patch(edit).delete(forget))
}

fn bad(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}
fn err(e: vortex_memory::MemError) -> ApiError {
    let code = match e {
        vortex_memory::MemError::NotFound(_) => StatusCode::NOT_FOUND,
        vortex_memory::MemError::Invalid(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ApiError::new(code, e.to_string())
}

#[derive(Debug, Deserialize, Default)]
struct ListQ {
    q: Option<String>,
    kind: Option<String>,
    workspace: Option<String>,
    #[serde(default)]
    stale: bool,
    #[serde(default = "lim")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}
fn lim() -> usize {
    50
}

fn filters(kind: Option<&str>, workspace: Option<String>, stale: bool) -> Filters {
    Filters {
        kinds: kind.and_then(MemoryKind::parse).into_iter().collect(),
        workspace,
        include_stale: stale,
        ..Default::default()
    }
}

async fn list(State(s): State<AppState>, AxQuery(q): AxQuery<ListQ>) -> Result<Json<Value>, ApiError> {
    let f = filters(q.kind.as_deref(), q.workspace, q.stale);
    Ok(Json(json!(s.memory.store.list(&f, q.q.as_deref(), q.limit.min(500), q.offset).map_err(err)?)))
}

async fn stats(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let mut v = s.memory.store.stats().map_err(err)?;
    v["embedder"] = json!(s.memory.store.config().dims);
    v["reranker"] = json!(s.memory.reranker.is_some());
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    content: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

async fn create(State(s): State<AppState>, Json(b): Json<CreateBody>) -> Result<Json<Value>, ApiError> {
    let kind = b.kind.as_deref().and_then(MemoryKind::parse).unwrap_or(MemoryKind::User);
    let mut m = NewMemory::new(kind, b.content, Provenance::with_origin(SourceKind::User, "memory-pagina"));
    m.workspace = b.workspace;
    m.title = b.title.unwrap_or_default();
    m.force = true; // gebruiker zegt expliciet "onthoud dit"
    Ok(Json(json!(s.memory.store.write_or_defer(m).await.map_err(err)?)))
}

#[derive(Debug, Deserialize)]
struct SearchBody {
    text: String,
    #[serde(default)]
    k: Option<usize>,
    #[serde(default)]
    workspace: Option<String>,
}

async fn search(State(s): State<AppState>, Json(b): Json<SearchBody>) -> Result<Json<Value>, ApiError> {
    let mut q = Query::new(b.text);
    q.k = b.k.unwrap_or(8).min(50);
    q.filters.workspace = b.workspace;
    Ok(Json(json!(s.memory.store.retrieve(&q, s.memory.reranker.as_ref()).await.map_err(err)?)))
}

async fn one(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let id: MemoryId = id.parse().map_err(bad)?;
    let m = s.memory.store.get(id).map_err(err)?;
    let versions = s.memory.store.versions(id).map_err(err)?;
    Ok(Json(json!({"memory": m, "versions": versions})))
}

#[derive(Debug, Deserialize)]
struct EditBody {
    content: String,
}

async fn edit(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<EditBody>,
) -> Result<Json<Value>, ApiError> {
    let id: MemoryId = id.parse().map_err(bad)?;
    Ok(Json(json!(s.memory.store.update(id, &b.content, "bewerkt in GUI").await.map_err(err)?)))
}

async fn forget(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let id: MemoryId = id.parse().map_err(bad)?;
    Ok(Json(json!({"forgotten": s.memory.store.forget(id).map_err(err)?})))
}

#[derive(Debug, Deserialize, Default)]
struct ConfQ {
    status: Option<String>,
}

async fn conflicts(State(s): State<AppState>, AxQuery(q): AxQuery<ConfQ>) -> Result<Json<Value>, ApiError> {
    let cs = s.memory.store.conflicts(q.status.as_deref()).map_err(err)?;
    let rows: Vec<Value> = cs
        .into_iter()
        .map(|c| {
            let a = s.memory.store.get(c.a).ok();
            let b = s.memory.store.get(c.b).ok();
            json!({"conflict": c, "a": a, "b": b})
        })
        .collect();
    Ok(Json(json!(rows)))
}

#[derive(Debug, Deserialize)]
struct ResolveBody {
    winner: Option<String>,
    #[serde(default)]
    note: String,
}

async fn resolve(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<ResolveBody>,
) -> Result<Json<Value>, ApiError> {
    let w = b.winner.map(|w| w.parse::<MemoryId>()).transpose().map_err(bad)?;
    s.memory.store.resolve_conflict(&id, w, &b.note).map_err(err)?;
    Ok(Json(json!({"ok": true})))
}

async fn workspaces(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.workspaces().map_err(err)?)))
}

async fn export(State(s): State<AppState>) -> Result<Response, ApiError> {
    let mut buf = Vec::new();
    s.memory.store.export(&mut buf, None).map_err(err)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"vortex-geheugen.jsonl\""),
        ],
        buf,
    )
        .into_response())
}

async fn import(State(s): State<AppState>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let n = s.memory.store.import(&mut std::io::Cursor::new(body.to_vec())).await.map_err(err)?;
    Ok(Json(json!({"imported": n})))
}

#[derive(Debug, Deserialize, Default)]
struct BackupBody {
    #[serde(default)]
    path: Option<String>,
}

async fn backup(State(s): State<AppState>, body: Option<Json<BackupBody>>) -> Result<Json<Value>, ApiError> {
    // Standaard naar de koude schijf uit config/paths.toml (cold_dir), anders data/backups.
    let cfg = s.kernel.config().current();
    let cold: Option<String> = cfg.get("paths.cold_dir").ok().flatten();
    let base = body.and_then(|b| b.0.path).map(std::path::PathBuf::from).unwrap_or_else(|| {
        cold.map(|c| std::path::PathBuf::from(c).join("backups"))
            .unwrap_or_else(|| std::path::PathBuf::from("data/backups"))
    });
    let file = base.join(format!("vortex-{}.db", vortex_types::now().format("%Y%m%d-%H%M%S")));
    s.memory.store.backup(&file).map_err(err)?;
    Ok(Json(json!({"path": file})))
}
