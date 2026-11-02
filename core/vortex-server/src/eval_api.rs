//! Eval vanuit de server: `VortexSubject` (Vortex zelf als testsubject, zonder blijvende opslag,
//! geïsoleerde eval-werkruimtes) + endpoints. VX:W06.01
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path as AxPath, Query as AxQuery, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_eval::{Runner, Subject};
use vortex_memory::{Filters, MemoryKind, NewMemory};
use vortex_types::{Provenance, SourceKind};

use crate::api::{ApiError, AppState};
use crate::chat::Mode;

/// Waves waarvan de functies in deze build aanwezig zijn (voor `requires` in eval-taken).
pub const FEATURES: &[&str] =
    &["W01", "W02", "W03", "W04", "W05", "W06", "W07", "W08", "W09", "W10", "W11", "W12", "W13"];

#[derive(Debug)]
pub struct VortexSubject {
    pub state: AppState,
    /// Modus voor álle taken (bv. "quick" = single-agent vs "deep" = debatteam, W09-gate).
    pub mode_override: Option<String>,
}

#[async_trait::async_trait]
impl Subject for VortexSubject {
    fn version(&self) -> String {
        let model = self.state.chat.registry().map(|r| r.default).unwrap_or_default();
        format!("{}+{}+{model}", env!("VORTEX_BUILD"), self.state.kernel.config().current().version)
    }
    fn has(&self, feature: &str) -> bool {
        FEATURES.contains(&feature)
    }
    async fn prepare(&self, ws: &str, facts: &[String]) -> Result<(), String> {
        for f in facts {
            let mut m =
                NewMemory::new(MemoryKind::User, f.clone(), Provenance::with_origin(SourceKind::Dataset, "eval"));
            m.workspace = Some(ws.to_string());
            m.force = true;
            self.state.memory.store.write(m).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    async fn answer(&self, ws: &str, prompt: &str, mode: Option<&str>) -> Result<String, String> {
        self.state
            .chat
            .answer_once(
                prompt,
                Mode::parse(self.mode_override.as_deref().or(mode).unwrap_or("adaptive")),
                Some(ws.to_string()),
            )
            .await
            .map_err(|e| e.to_string())
    }
    async fn open_conflicts(&self, ws: &str) -> usize {
        let store = &self.state.memory.store;
        store
            .conflicts(Some("open"))
            .unwrap_or_default()
            .into_iter()
            .filter(|c| store.get(c.a).ok().and_then(|m| m.workspace).as_deref() == Some(ws))
            .count()
    }
    async fn cleanup(&self, ws: &str) {
        let store = &self.state.memory.store;
        let f = Filters { workspace: Some(ws.to_string()), include_stale: true, ..Default::default() };
        for m in store.list(&f, None, 10_000, 0).unwrap_or_default() {
            if m.workspace.as_deref() == Some(ws) {
                let _ = store.forget(m.id);
            }
        }
    }
}

pub fn evals_dir() -> PathBuf {
    std::env::var("VORTEX_EVALS").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("../evals"))
}

pub(crate) fn python() -> String {
    if std::process::Command::new("python3").arg("-V").output().is_ok() { "python3".into() } else { "python".into() }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/eval/suites", get(suites))
        .route("/api/eval/run", post(run))
        .route("/api/eval/runs", get(runs))
        .route("/api/eval/runs/:id", get(run_detail))
        .route("/api/eval/retrieval", post(retrieval))
        .route("/api/eval/leakage", get(leakage))
}

fn e500(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn suites() -> Result<Json<Value>, ApiError> {
    let s = vortex_eval::load_dir(&evals_dir().join("sets")).map_err(e500)?;
    Ok(Json(json!(s.iter().map(|x| json!({"name": x.name, "tasks": x.tasks.len()})).collect::<Vec<_>>())))
}

#[derive(Debug, Deserialize)]
struct RunBody {
    #[serde(default)]
    suite: Option<String>,
    #[serde(default)]
    label: Option<String>,
    /// "quick" (single-agent) of "deep" (debatteam) voor alle taken.
    #[serde(default)]
    mode: Option<String>,
}

/// Start een eval op de achtergrond (P2 via de Dirigent); voortgang via events.
async fn run(State(s): State<AppState>, Json(b): Json<RunBody>) -> Result<Json<Value>, ApiError> {
    let all = vortex_eval::load_dir(&evals_dir().join("sets")).map_err(e500)?;
    let chosen: Vec<_> =
        all.into_iter().filter(|x| b.suite.as_deref().is_none_or(|n| n == "all" || n == x.name)).collect();
    if chosen.is_empty() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "geen suite gevonden"));
    }
    let names: Vec<String> = chosen.iter().map(|x| x.name.clone()).collect();
    let st = s.clone();
    tokio::spawn(async move {
        let subject = VortexSubject { state: st.clone(), mode_override: b.mode.clone() };
        let runner = Runner { log: st.kernel.events(), subject: &subject, python: python() };
        for suite in chosen {
            let label = match (&b.label, &b.mode) {
                (Some(l), _) => Some(l.clone()),
                (None, Some(m)) => Some(format!("modus {m}")),
                _ => None,
            };
            match runner.run_suite(&suite, label.as_deref()).await {
                Ok(r) => {
                    tracing::info!(suite = %r.suite, score = ?r.score, "eval klaar");
                    st.chat.refresh_self_model();
                }
                Err(e) => tracing::error!(error = %e, "eval mislukt"),
            }
        }
    });
    Ok(Json(json!({"started": names})))
}

#[derive(Debug, Deserialize, Default)]
struct RunsQ {
    suite: Option<String>,
}

async fn runs(State(s): State<AppState>, AxQuery(q): AxQuery<RunsQ>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(vortex_eval::list_runs(s.kernel.events(), q.suite.as_deref(), 200).map_err(e500)?)))
}

async fn run_detail(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let cmp = vortex_eval::compare(s.kernel.events(), &id).map_err(e500)?;
    let res = vortex_eval::results(s.kernel.events(), &id).map_err(e500)?;
    Ok(Json(json!({"comparison": cmp, "results": res})))
}

/// Retrieval-eval met het huidige embeddingmodel (geïsoleerd geheugen).
async fn retrieval(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let text = std::fs::read_to_string(evals_dir().join("retrieval.json")).map_err(e500)?;
    let set: vortex_eval::RetrievalSet = serde_json::from_str(&text).map_err(e500)?;
    let store = &s.memory.store;
    let rep = vortex_eval::retrieval::run(
        &set,
        store_embedder(store),
        Arc::new(vortex_memory::HeuristicLlm),
        s.memory.reranker.clone(),
    )
    .await
    .map_err(e500)?;
    Ok(Json(json!(rep)))
}

fn store_embedder(store: &vortex_memory::MemoryStore) -> Arc<dyn vortex_memory::Embedder> {
    store.embedder()
}

/// Lekkagecontrole: eval-vragen en -feiten tegen het echte geheugen.
async fn leakage(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let suites = vortex_eval::load_dir(&evals_dir().join("sets")).map_err(e500)?;
    let mut evals = Vec::new();
    for su in &suites {
        for t in &su.tasks {
            evals.push((format!("{}/{}", su.name, t.id), t.prompt.clone()));
            for (i, f) in t.memory.iter().enumerate() {
                evals.push((format!("{}/{}#feit{i}", su.name, t.id), f.clone()));
            }
        }
    }
    let f = Filters { include_stale: true, ..Default::default() };
    let corpus: Vec<(String, String)> = s
        .memory
        .store
        .list(&f, None, 100_000, 0)
        .map_err(e500)?
        .into_iter()
        .filter(|m| !m.workspace.as_deref().unwrap_or("").starts_with("__eval"))
        .map(|m| (m.id.to_string(), m.content))
        .collect();
    let leaks = vortex_eval::leakage::check(&evals, &corpus, 0.6);
    Ok(Json(json!({"checked_evals": evals.len(), "checked_memories": corpus.len(), "leaks": leaks})))
}
