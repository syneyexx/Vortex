//! Debatteam + conflict-team in de server (W09): LLM per rol, conflict-team op de voorgrond
//! (in de chat) en op de achtergrond (worker), REST om een conflict door het team te laten
//! beslissen of een beslissing terug te draaien.
// VX:W09.09

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_agents::conflict::{ConflictDeps, resolve};
use vortex_agents::{Reporter, Roles};
use vortex_cognition::CogLlm;
use vortex_core::{ConfigSnapshot, RunSpec};
use vortex_memory::Resolution;
use vortex_types::{Priority, RunKind};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;
use crate::cognitive::BackendLlm;

impl ChatEngine {
    /// LLM voor een modelrol uit models.toml ("extract" = 1060, "chat" = hoofdmodel).
    pub(crate) fn role_llm(
        &self,
        cfg: &ConfigSnapshot,
        role: &str,
        cancel: CancellationToken,
    ) -> Option<Arc<dyn CogLlm>> {
        let reg = vortex_models::Registry::from_config(cfg).ok()?;
        let name = match role {
            "chat" => reg.default.clone(),
            r => reg.for_role(r).map(|(n, _)| n.to_string())?,
        };
        let backend = self.backends.for_model(&reg, &name).ok()?;
        Some(Arc::new(BackendLlm { backend, cancel }))
    }

    /// Conflict door het team laten beslissen (als eigen run, zichtbaar in Lopend werk).
    pub async fn resolve_conflict_team(&self, conflict_id: &str, priority: Priority) -> Result<Resolution, String> {
        let mem = self.memory.clone().ok_or("geen geheugen")?;
        let c = mem
            .store
            .conflicts(None)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|c| c.id == conflict_id)
            .ok_or("conflict onbekend")?;
        let mut spec = RunSpec::new(RunKind::Maintenance, priority);
        spec.input = json!({"conflict": conflict_id, "team": "conflict"});
        let ctx = self.kernel.start_run(spec).map_err(|e| e.to_string())?;
        ctx.start().map_err(|e| e.to_string())?;
        let r = async {
            let _gpu = ctx.gpu(priority).await.map_err(|e| e.to_string())?;
            let llm =
                self.role_llm(ctx.config(), "chat", ctx.token().child_token()).ok_or("geen model geconfigureerd")?;
            let deps = ConflictDeps {
                llm,
                roles: Roles::from_snapshot(ctx.config()),
                reporter: Reporter::new(Some(self.kernel.events().clone()), Some(ctx.id()), ctx.trace().trace_id),
            };
            resolve(&deps, &mem.store, &c).await
        }
        .await;
        match &r {
            Ok(res) => {
                let _ = ctx.complete(json!({"resolution": res}));
            }
            Err(e) => {
                let _ = ctx.fail(e);
            }
        }
        r
    }
}

/// Achtergrond: open conflicten die niet in een gesprek opkwamen, rustig (P3) laten beslissen.
pub fn spawn_conflict_worker(chat: ChatEngine, stop: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(90)) => {}
            }
            if !crate::eval_api::FEATURES.contains(&"W09") {
                continue;
            }
            let Some(mem) = &chat.memory else { continue };
            let open = mem.store.conflicts(Some("open")).unwrap_or_default();
            for c in open.iter().rev().take(2) {
                match chat.resolve_conflict_team(&c.id, Priority::P3).await {
                    Ok(r) => tracing::info!(conflict = %c.id, ?r, "conflict op de achtergrond beslist"),
                    Err(e) => {
                        tracing::debug!(conflict = %c.id, error = %e, "conflict-team kon niet beslissen");
                        break;
                    }
                }
            }
        }
    });
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/memory/conflicts/:id/team", post(team_resolve))
        .route("/api/memory/conflicts/:id/rollback", post(rollback))
}

async fn team_resolve(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let r =
        s.chat.resolve_conflict_team(&id, Priority::P1).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({"resolution": r})))
}

async fn rollback(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let n = s
        .memory
        .store
        .rollback_conflict(&id)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"restored": n})))
}
