//! Geheugen gevorderd in de server (W12): zelfmodel uit eval, dromen + tiering in idle,
//! procedures uit skills, few-shot-voorbeelden, media-referenties en de API's (graph, dromen,
//! meta, tiering, kansmodellen-experiment).
// VX:W12.03

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_cognition::SelfModel;
use vortex_core::RunSpec;
use vortex_memory::dream::DreamOptions;
use vortex_memory::graph::{Rule, default_rules};
use vortex_memory::procedural::Procedure;
use vortex_memory::tiering::TierConfig;
use vortex_memory::{MemoryKind, NewMemory};
use vortex_types::{Priority, Provenance, RunId, RunKind, SourceKind};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

fn e500(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

impl ChatEngine {
    /// Vraag (gebruikersbericht) van een run.
    pub fn question_of(&self, run: RunId) -> Option<String> {
        self.kernel
            .events()
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT content FROM messages WHERE run_id = ?1 AND role = 'user' LIMIT 1",
                    [run.to_string()],
                    |r| r.get::<_, String>(0),
                )
                .ok())
            })
            .ok()
            .flatten()
    }

    /// Regels voor graph-redeneren: standaard + `config/rules.toml` (`[[rules]]`).
    pub fn graph_rules(&self) -> Vec<Rule> {
        let mut r = default_rules();
        if let Ok(Some(extra)) = self.kernel.config().current().get::<Vec<Rule>>("rules.rules") {
            r.extend(extra);
        }
        r
    }

    /// Zelfmodel uit de laatste eval-run per suite (W12.06) + zwakke taaktypen → leeragenda.
    pub fn refresh_self_model(&self) -> SelfModel {
        let sets = vortex_eval::load_dir(&crate::eval_api::evals_dir().join("sets")).unwrap_or_default();
        let prompts: std::collections::HashMap<String, String> = sets
            .into_iter()
            .flat_map(|s| s.tasks.into_iter().map(move |t| (format!("{}/{}", s.name, t.id), t.prompt)))
            .collect();
        let rows: Vec<(String, String, f64)> = self
            .kernel
            .events()
            .db()
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT r.suite, e.task_id, e.score FROM eval_results e JOIN eval_runs r ON r.id = e.run_id
                     WHERE e.status IN ('pass','fail') AND r.started_at = (SELECT MAX(r2.started_at) FROM eval_runs r2 WHERE r2.suite = r.suite)",
                )?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let items: Vec<(String, f64)> = rows
            .into_iter()
            .filter_map(|(s, t, sc)| prompts.get(&format!("{s}/{t}")).map(|p| (p.clone(), sc)))
            .collect();
        let m = SelfModel::from_results(&items);
        for (task, sc) in m.weak() {
            let _ = self.agenda.add(
                &format!("zwak taaktype: {task}"),
                "weak_task",
                &format!("eval-score {:.0}%", sc * 100.0),
                0.6,
            );
        }
        *self.self_model.write() = m.clone();
        m
    }

    /// Procedures uit skills (+ statistiek) naar procedural memory (W12.01).
    pub async fn sync_procedures(&self) -> usize {
        let (Some(t), Some(m)) = (&self.tools, &self.memory) else { return 0 };
        let procs: Vec<Procedure> = t
            .skills
            .list()
            .into_iter()
            .map(|s| {
                let st = t.skills.stats(&s.name).unwrap_or_default();
                let total = (st.successes + st.failures).max(0);
                Procedure {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    body: s.body.clone(),
                    success_rate: if total > 0 { st.successes as f64 / total as f64 } else { 0.5 },
                    uses: st.uses,
                    version: s.version.clone(),
                    origin: if s.origin == "proposed" { "werkwijze".into() } else { "skill".into() },
                }
            })
            .collect();
        m.store.sync_procedures(&procs).await.unwrap_or(0)
    }

    /// Dromen als run (P3) + daarna gaten → leeragenda, procedures bijwerken, koude laag vullen.
    pub async fn dream_now(&self, priority: Priority) -> Result<Value, String> {
        let mem = self.memory.clone().ok_or("geen geheugen")?;
        let mut spec = RunSpec::new(RunKind::Maintenance, priority);
        spec.input = json!({"job": "dream"});
        let ctx = self.kernel.start_run(spec).map_err(|e| e.to_string())?;
        ctx.start().map_err(|e| e.to_string())?;
        let r = async {
            let _gpu = ctx.gpu(priority).await.map_err(|e| e.to_string())?;
            let o = DreamOptions { rules: self.graph_rules(), ..Default::default() };
            let rep = mem.store.dream(&o).await.map_err(|e| e.to_string())?;
            let meta = mem.store.meta_report().map_err(|e| e.to_string())?;
            for g in &meta.gaps {
                let _ = self.agenda.add(&g.name, "gap", &format!("{}× genoemd, {} feiten", g.mentions, g.facts), 0.5);
            }
            let procs = self.sync_procedures().await;
            let cold = match mem.store.cold_candidates(500) {
                Ok(ids) if !ids.is_empty() => mem.store.demote(&ids).unwrap_or(0),
                _ => 0,
            };
            Ok::<_, String>(json!({"dream": rep, "gaps": meta.gaps.len(), "procedures": procs, "demoted": cold}))
        }
        .await;
        match &r {
            Ok(v) => {
                let _ = ctx.complete(v.clone());
            }
            Err(e) => {
                let _ = ctx.fail(e);
            }
        }
        r
    }
}

/// Tiering instellen: koude schijf uit `paths.cold_dir`, SSD-cache in de datamap.
pub fn attach(chat: &ChatEngine) {
    if let Some(m) = &chat.memory {
        let cold = crate::research_api::cold_dir(&chat.kernel.config().current(), &chat.data_dir()).join("memory");
        m.store.set_tiering(Some(TierConfig::new(cold, chat.data_dir().join("cache").join("cold"))));
    }
    chat.refresh_self_model();
}

/// Idle: elke 15 min kijken; ≥ 6 uur na de vorige droom en de gebruiker is idle → dromen.
pub fn spawn_dreaming(chat: ChatEngine, stop: CancellationToken) {
    tokio::spawn(async move {
        let _ = chat.sync_procedures().await;
        let mut last = std::time::Instant::now() - Duration::from_secs(6 * 3600);
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(900)) => {}
            }
            if !crate::eval_api::FEATURES.contains(&"W12")
                || !chat.kernel.dirigent().conditions().user_idle
                || last.elapsed() < Duration::from_secs(6 * 3600)
            {
                continue;
            }
            last = std::time::Instant::now();
            match chat.dream_now(Priority::P3).await {
                Ok(v) => tracing::info!(report = %v, "gedroomd"),
                Err(e) => tracing::warn!(error = %e, "dromen mislukt"),
            }
        }
    });
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/memory/graph", get(graph))
        .route("/api/memory/graph/history", get(history))
        .route("/api/memory/reason", post(reason))
        .route("/api/memory/dream", post(dream))
        .route("/api/memory/dreams", get(dreams))
        .route("/api/memory/meta", get(meta))
        .route("/api/memory/procedures", get(procedures))
        .route("/api/memory/tier", get(tier))
        .route("/api/memory/tier/demote", post(demote))
        .route("/api/memory/media", post(media_add))
        .route("/api/memory/:id/media", get(media_get))
        .route("/api/memory/belief-experiment", get(belief))
        .route("/api/selfmodel", get(selfmodel))
}

#[derive(Debug, Deserialize)]
struct GraphQ {
    entity: String,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    rel: Option<String>,
}

async fn graph(State(s): State<AppState>, Query(q): Query<GraphQ>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.facts_about(&q.entity, q.at.as_deref()).map_err(e500)?)))
}

async fn history(State(s): State<AppState>, Query(q): Query<GraphQ>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.fact_history(&q.entity, q.rel.as_deref()).map_err(e500)?)))
}

async fn reason(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.reason(&s.chat.graph_rules()).map_err(e500)?)))
}

async fn dream(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.chat.dream_now(Priority::P2).await.map_err(e500)?))
}

async fn dreams(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.dream_runs(30).map_err(e500)?)))
}

async fn meta(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.meta_report().map_err(e500)?)))
}

async fn procedures(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    s.chat.sync_procedures().await;
    Ok(Json(json!(s.memory.store.procedures().map_err(e500)?)))
}

async fn tier(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.memory.store.tier_stats().map_err(e500)?)))
}

#[derive(Debug, Deserialize)]
struct DemoteBody {
    #[serde(default)]
    max: Option<usize>,
    /// Expliciete ID's (anders: decay-gestuurde kandidaten).
    #[serde(default)]
    ids: Vec<String>,
}

async fn demote(State(s): State<AppState>, Json(b): Json<DemoteBody>) -> Result<Json<Value>, ApiError> {
    let ids = if b.ids.is_empty() {
        s.memory.store.cold_candidates(b.max.unwrap_or(500)).map_err(e500)?
    } else {
        b.ids.iter().filter_map(|i| i.parse().ok()).collect()
    };
    Ok(Json(json!({"demoted": s.memory.store.demote(&ids).map_err(e500)?})))
}

#[derive(Debug, Deserialize)]
struct MediaBody {
    path: String,
    description: String,
    #[serde(default)]
    workspace: Option<String>,
}

/// Afbeelding/bestand als referentie (W12.08): beschrijving wordt geëmbed; het bestand blijft
/// waar het staat (of op de koude schijf). Beeld-embeddings (CLIP) volgen in W16.
async fn media_add(State(s): State<AppState>, Json(b): Json<MediaBody>) -> Result<Json<Value>, ApiError> {
    let p = PathBuf::from(&b.path);
    let meta = std::fs::metadata(&p).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("{}: {e}", b.path)))?;
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let kind = if ["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str()) {
        "image"
    } else if ["mp3", "wav", "ogg", "m4a"].contains(&ext.as_str()) {
        "audio"
    } else if ["mp4", "mkv", "webm", "mov"].contains(&ext.as_str()) {
        "video"
    } else {
        "file"
    };
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let mut nm = NewMemory::new(
        MemoryKind::Semantic,
        format!("{} ({kind}: {name}, {} KB)", b.description.trim(), meta.len() / 1024),
        Provenance::with_origin(SourceKind::File, b.path.clone()),
    );
    nm.title = format!("{kind}: {name}");
    nm.source_uri = Some(std::fs::canonicalize(&p).unwrap_or(p).display().to_string());
    nm.tags = vec!["media".into(), kind.into(), ext];
    nm.workspace = b.workspace;
    nm.force = true;
    Ok(Json(json!(s.memory.store.write_or_defer(nm).await.map_err(e500)?)))
}

async fn media_get(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Response, ApiError> {
    let mid = id.parse().map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "ongeldig ID"))?;
    let m = s.memory.store.get(mid).map_err(|e| ApiError::new(StatusCode::NOT_FOUND, e.to_string()))?;
    if !m.tags.iter().any(|t| t == "media") {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "geen media"));
    }
    let path = m.source_uri.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "geen pad"))?;
    let bytes = std::fs::read(&path).map_err(|e| ApiError::new(StatusCode::NOT_FOUND, e.to_string()))?;
    let ct = match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, ct)], Body::from(bytes)).into_response())
}

#[derive(Debug, Deserialize)]
struct BeliefQ {
    #[serde(default)]
    n: Option<usize>,
}

async fn belief(Query(q): Query<BeliefQ>) -> Json<Value> {
    Json(json!(vortex_memory::belief::experiment(q.n.unwrap_or(400).min(5000), 7)))
}

async fn selfmodel(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.chat.refresh_self_model()))
}
