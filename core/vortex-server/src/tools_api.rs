//! Tools en skills in de server (W08): hub bouwen uit de config, geheugen-tools registreren,
//! REST voor de GUI (tools, handmatig aanroepen met preview, skills importeren/exporteren,
//! voorstellen accepteren, herladen).
// VX:W08.07

use std::sync::Arc;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_core::Kernel;
use vortex_memory::{MemoryKind, NewMemory, WriteOutcome};
use vortex_tools::contract::{Effect, ToolContract};
use vortex_tools::{
    CapabilityHandle, ExecOptions, Tool, ToolCtx, ToolError, ToolErrorKind, ToolHub, ToolHubConfig, ToolOutput,
};
use vortex_types::{Provenance, SourceKind};

use crate::api::{ApiError, AppState};
use crate::memory_api::MemoryHandle;

type R<T> = Result<Json<T>, ApiError>;

/// Hub bouwen + geheugen-tools + MCP op de achtergrond verbinden.
pub fn build(kernel: &Kernel, memory: Option<MemoryHandle>) -> Arc<ToolHub> {
    let snap = kernel.config().current();
    let data_dir =
        kernel.db().path().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_else(std::env::temp_dir);
    let cfg = ToolHubConfig::from_snapshot(&snap, kernel.config().dir(), &data_dir);
    let secrets = kernel.secrets().clone();
    let hub = ToolHub::new(
        cfg,
        kernel.events().clone(),
        kernel.supervisor().clone(),
        Arc::new(move |n: &str| secrets.get(n).ok().flatten().map(|s| s.expose().to_string())),
    );
    if let Some(m) = memory {
        hub.registry.register(Arc::new(MemorySearch::new(m.clone())));
        hub.registry.register(Arc::new(MemoryWrite::new(m.clone())));
        hub.registry.register(Arc::new(MemoryForget::new(m)));
    }
    // W17.14: browser en computer use via de worker.
    hub.registry.register(Arc::new(crate::automation::Browser::new(kernel.clone())));
    hub.registry.register(Arc::new(crate::automation::Computer::new(kernel.clone())));
    if !hub.cfg.mcp.servers.is_empty() {
        let h = hub.clone();
        tokio::spawn(async move {
            let st = h.connect_mcp().await;
            tracing::info!(?st, "MCP-servers");
        });
    }
    hub
}

// ---------- geheugen-tools ----------

#[derive(Debug)]
struct MemorySearch {
    c: ToolContract,
    m: MemoryHandle,
}
impl MemorySearch {
    fn new(m: MemoryHandle) -> Self {
        Self {
            c: ToolContract::new(
                "memory_search",
                "Zoek in het geheugen van Vortex (feiten over de gebruiker, projecten, eerdere gesprekken, kennis).",
                Effect::Read,
                json!({"type": "object", "required": ["query"], "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "k": {"type": "integer", "minimum": 1, "maximum": 30, "default": 6},
                    "kinds": {"type": "array", "items": {"type": "string", "enum": ["user", "project", "semantic", "episodic", "world", "procedural", "summary"]}}
                }}),
            )
            .origin("server")
            .timeout(10_000),
            m,
        }
    }
}
#[async_trait::async_trait]
impl Tool for MemorySearch {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        let mut q = vortex_memory::Query::new(i["query"].as_str().unwrap_or("").to_string());
        q.k = i["k"].as_u64().unwrap_or(6) as usize;
        q.run_id = ctx.run;
        if let Some(ks) = i["kinds"].as_array() {
            q.filters.kinds = ks.iter().filter_map(|k| k.as_str().and_then(MemoryKind::parse)).collect();
        }
        let items =
            self.m.store.retrieve(&q, self.m.reranker.as_ref()).await.map_err(|e| ToolError::failed(e.to_string()))?;
        let rows: Vec<Value> = items
            .iter()
            .map(|r| json!({"id": r.memory.id.to_string(), "kind": r.memory.kind.as_str(), "text": r.memory.content.chars().take(600).collect::<String>(), "score": r.score.final_score, "counter_evidence": r.counter_evidence}))
            .collect();
        let text = if rows.is_empty() {
            "Niets gevonden in het geheugen.".to_string()
        } else {
            rows.iter()
                .map(|r| {
                    format!(
                        "[{} {} {:.2}{}] {}",
                        r["id"].as_str().unwrap_or(""),
                        r["kind"].as_str().unwrap_or(""),
                        r["score"].as_f64().unwrap_or(0.0),
                        if r["counter_evidence"] == true { " TEGENBEWIJS" } else { "" },
                        r["text"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::json(json!({"items": rows})).with_text(text))
    }
}

#[derive(Debug)]
struct MemoryWrite {
    c: ToolContract,
    m: MemoryHandle,
}
impl MemoryWrite {
    fn new(m: MemoryHandle) -> Self {
        Self {
            c: ToolContract::new(
                "memory_write",
                "Sla iets op in het geheugen (feit over de gebruiker, projectafspraak, kennis).",
                Effect::Local,
                json!({"type": "object", "required": ["content"], "properties": {
                    "content": {"type": "string", "minLength": 3},
                    "kind": {"type": "string", "enum": ["user", "project", "semantic", "world", "procedural"], "default": "semantic"},
                    "title": {"type": "string"}
                }}),
            )
            .origin("server")
            .timeout(30_000),
            m,
        }
    }
}
#[async_trait::async_trait]
impl Tool for MemoryWrite {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        let kind = MemoryKind::parse(i["kind"].as_str().unwrap_or("semantic")).unwrap_or(MemoryKind::Semantic);
        let origin = ctx.run.map(|r| format!("run:{r}")).unwrap_or_else(|| "tool".into());
        let mut nm = NewMemory::new(
            kind,
            i["content"].as_str().unwrap_or("").to_string(),
            Provenance::with_origin(SourceKind::Tool, origin),
        );
        if let Some(t) = i["title"].as_str() {
            nm.title = t.to_string();
        }
        nm.force = true;
        let out = self.m.store.write(nm).await.map_err(|e| ToolError::failed(e.to_string()))?;
        let (ids, txt) = match out {
            WriteOutcome::Stored { ids, .. } => {
                (ids.iter().map(|i| i.to_string()).collect::<Vec<_>>(), "Opgeslagen".to_string())
            }
            WriteOutcome::Merged { into } => {
                (vec![into.to_string()], "Samengevoegd met een bestaande herinnering".to_string())
            }
            WriteOutcome::Skipped { reason, .. } => (vec![], format!("Niet opgeslagen: {reason}")),
        };
        Ok(ToolOutput::json(json!({"ids": ids})).with_text(format!("{txt} ({})", ids.join(", "))))
    }
}

#[derive(Debug)]
struct MemoryForget {
    c: ToolContract,
    m: MemoryHandle,
}
impl MemoryForget {
    fn new(m: MemoryHandle) -> Self {
        Self {
            c: ToolContract::new(
                "memory_forget",
                "Vergeet een herinnering (met alles wat ervan is afgeleid).",
                Effect::Local,
                json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "string"}}}),
            )
            .origin("server"),
            m,
        }
    }
}
#[async_trait::async_trait]
impl Tool for MemoryForget {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        let id = i["id"].as_str().unwrap_or("").parse().map_err(|_| ToolError::invalid("ongeldig herinnering-ID"))?;
        let gone = self.m.store.forget(id).map_err(|e| ToolError::new(ToolErrorKind::Precondition, e.to_string()))?;
        Ok(ToolOutput::json(json!({"forgotten": gone.len()}))
            .with_text(format!("Vergeten: {} herinnering(en)", gone.len())))
    }
}

// ---------- REST ----------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tools", get(list_tools))
        .route("/api/tools/reload", post(reload))
        .route("/api/tools/:name/call", post(call_tool))
        .route("/api/skills", get(list_skills))
        .route("/api/skills/import", post(import_skill))
        .route("/api/skills/:name", axum::routing::delete(delete_skill))
        .route("/api/skills/:name/export", post(export_skill))
        .route("/api/skills/:name/status", post(skill_status))
        .route("/api/skills/proposals", get(proposals))
        .route("/api/skills/proposals/:id", post(decide_proposal))
}

fn hub(s: &AppState) -> Result<&Arc<ToolHub>, ApiError> {
    s.tools.as_ref().ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "tools niet actief"))
}

async fn list_tools(State(s): State<AppState>) -> R<Value> {
    let h = hub(&s)?;
    let core = h.core_set();
    let tools: Vec<Value> = h
        .registry
        .contracts()
        .into_iter()
        .map(|c| json!({"name": c.name, "description": c.description, "effect": c.effect, "origin": c.origin, "timeout_ms": c.timeout_ms, "idempotent": c.idempotent, "core": core.contains(&c.name), "input_schema": c.input_schema}))
        .collect();
    Ok(Json(json!({"tools": tools, "mcp": h.mcp.status()})))
}

async fn reload(State(s): State<AppState>) -> R<Value> {
    let h = hub(&s)?;
    let n = h.skills.reload();
    h.executor.cache().clear();
    let mcp = h.connect_mcp().await;
    Ok(Json(json!({"skills": n, "mcp": mcp, "tools": h.registry.len()})))
}

#[derive(Debug, Deserialize)]
struct CallBody {
    #[serde(default)]
    input: Value,
    #[serde(default)]
    dry_run: bool,
    /// Beperk tot deze map (capability handle).
    #[serde(default)]
    root: Option<String>,
}

async fn call_tool(State(s): State<AppState>, AxPath(name): AxPath<String>, Json(b): Json<CallBody>) -> R<Value> {
    let h = hub(&s)?;
    let caps = match &b.root {
        Some(r) => CapabilityHandle::scoped(r),
        None => CapabilityHandle::default(),
    };
    let opts = ExecOptions { dry_run: b.dry_run, caps, ..Default::default() };
    let id = format!("manual:{}", vortex_types::EventId::new());
    let r = h.executor.call(&id, &name, b.input, &opts).await;
    Ok(Json(serde_json::to_value(&r).unwrap_or_default()))
}

async fn list_skills(State(s): State<AppState>) -> R<Value> {
    let h = hub(&s)?;
    let skills: Vec<Value> = h
        .skills
        .list()
        .into_iter()
        .map(|sk| {
            let st = h.skills.stats(&sk.name);
            json!({"name": sk.name, "description": sk.description, "origin": sk.origin, "version": sk.version, "globs": sk.globs, "always_apply": sk.always_apply, "scripts": sk.scripts, "stats": st})
        })
        .collect();
    Ok(Json(json!({"skills": skills, "dir": h.skills.dir()})))
}

#[derive(Debug, Deserialize)]
struct ImportBody {
    path: String,
}

async fn import_skill(State(s): State<AppState>, Json(b): Json<ImportBody>) -> R<Value> {
    let h = hub(&s)?;
    let found = vortex_tools::skills::convert::import(std::path::Path::new(&b.path))
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let mut names = Vec::new();
    for sk in &found {
        h.skills.install(sk).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        names.push(sk.name.clone());
    }
    Ok(Json(json!({"imported": names})))
}

#[derive(Debug, Deserialize)]
struct ExportBody {
    /// claude | cursor | agents
    format: String,
    dir: String,
}

async fn export_skill(State(s): State<AppState>, AxPath(name): AxPath<String>, Json(b): Json<ExportBody>) -> R<Value> {
    use vortex_tools::skills::convert::{export_agents_md, export_claude, export_cursor};
    let h = hub(&s)?;
    let sk = h.skills.get(&name).ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "skill onbekend"))?;
    let dir = std::path::Path::new(&b.dir);
    let p = match b.format.as_str() {
        "claude" => export_claude(&sk, dir),
        "cursor" => export_cursor(&sk, dir),
        "agents" => export_agents_md(&[sk], &dir.join("AGENTS.md")),
        f => return Err(ApiError::new(StatusCode::BAD_REQUEST, format!("onbekend formaat {f}"))),
    }
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"path": p})))
}

async fn delete_skill(State(s): State<AppState>, AxPath(name): AxPath<String>) -> R<Value> {
    hub(&s)?.skills.remove(&name).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
struct StatusBody {
    status: String,
}

async fn skill_status(State(s): State<AppState>, AxPath(name): AxPath<String>, Json(b): Json<StatusBody>) -> R<Value> {
    if !["active", "disabled"].contains(&b.status.as_str()) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "status: active | disabled"));
    }
    hub(&s)?.skills.set_status(&name, &b.status).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
struct PropQ {
    status: Option<String>,
}

async fn proposals(State(s): State<AppState>, Query(q): Query<PropQ>) -> R<Value> {
    hub(&s)?;
    Ok(Json(json!(vortex_tools::skills::propose::list(s.kernel.events(), q.status.as_deref().or(Some("proposed"))))))
}

async fn decide_proposal(State(s): State<AppState>, AxPath(id): AxPath<String>, Json(b): Json<StatusBody>) -> R<Value> {
    let h = hub(&s)?;
    if !["accepted", "rejected"].contains(&b.status.as_str()) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "status: accepted | rejected"));
    }
    let p = vortex_tools::skills::propose::set_status(s.kernel.events(), &id, &b.status)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "voorstel onbekend"))?;
    if b.status == "accepted" {
        h.skills
            .install(&vortex_tools::skills::propose::to_skill(&p))
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    Ok(Json(json!({"ok": true, "name": p.name})))
}
