//! REST voor autonomie: taken, projecten, triggers, suggesties, briefings, backups, GPU,
//! voortgang en de "Autonomie"-status.

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use super::queue::NewTask;
use super::{backup, briefing, monitor, progress};
use crate::api::{ApiError, AppState};

fn e400(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}

#[derive(Debug, Deserialize)]
struct TaskQ {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    project: Option<String>,
}

async fn tasks(State(s): State<AppState>, Query(q): Query<TaskQ>) -> Json<Value> {
    Json(json!(s.autonomy.queue.list(q.status.as_deref(), q.project.as_deref(), 300)))
}

async fn add_task(State(s): State<AppState>, Json(b): Json<NewTask>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.autonomy.queue.enqueue(b).map_err(e400)?)))
}

async fn task(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    s.autonomy
        .queue
        .get(&id)
        .map(|t| Json(json!(t)))
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onbekende taak"))
}

async fn task_action(
    State(s): State<AppState>,
    AxPath((id, action)): AxPath<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let a = &s.autonomy;
    let ok = match action.as_str() {
        "cancel" => a.cancel(&id),
        "pause" => a.pause(&id),
        "resume" => a.queue.resume(&id),
        _ => return Err(e400("actie = cancel | pause | resume")),
    };
    Ok(Json(json!({"ok": ok, "task": a.queue.get(&id)})))
}

async fn projects(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.autonomy.queue.projects()))
}

#[derive(Debug, Deserialize)]
struct NewProject {
    name: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    workspace: Option<String>,
}

async fn add_project(State(s): State<AppState>, Json(b): Json<NewProject>) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.autonomy.queue.create_project(&b.name, &b.goal, b.workspace.as_deref()).map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct ProjectUpd {
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

async fn update_project(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<ProjectUpd>,
) -> Result<Json<Value>, ApiError> {
    let q = &s.autonomy.queue;
    if let Some(n) = b.note {
        q.note(&id, &n);
    }
    if let Some(st) = b.status {
        q.set_project_status(&id, &st).map_err(e400)?;
    }
    q.project(&id).map(Json).ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onbekend project"))
}

async fn project(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    s.autonomy.queue.project(&id).map(Json).ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onbekend project"))
}

async fn triggers(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.autonomy.triggers.list()))
}

#[derive(Debug, Deserialize)]
struct NewTrigger {
    name: String,
    kind: String,
    spec: String,
    task: NewTask,
}

async fn add_trigger(State(s): State<AppState>, Json(b): Json<NewTrigger>) -> Result<Json<Value>, ApiError> {
    let id = s.autonomy.triggers.add(&b.name, &b.kind, &b.spec, &b.task, false).map_err(e400)?;
    Ok(Json(json!({"id": id})))
}

#[derive(Debug, Deserialize)]
struct TrigUpd {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    remove: bool,
}

async fn update_trigger(State(s): State<AppState>, AxPath(id): AxPath<String>, Json(b): Json<TrigUpd>) -> Json<Value> {
    let t = &s.autonomy.triggers;
    let ok = if b.remove { t.remove(&id) } else { b.enabled.is_some_and(|e| t.set_enabled(&id, e)) };
    Json(json!({"ok": ok}))
}

#[derive(Debug, Deserialize)]
struct SugQ {
    #[serde(default)]
    status: Option<String>,
}

async fn suggestions(State(s): State<AppState>, Query(q): Query<SugQ>) -> Json<Value> {
    Json(json!(monitor::list(s.kernel.events(), q.status.as_deref())))
}

async fn decide(
    State(s): State<AppState>,
    AxPath((id, action)): AxPath<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(monitor::decide(&s, &id, action == "accept").map_err(e400)?))
}

async fn scan(State(s): State<AppState>) -> Json<Value> {
    Json(monitor::scan(&s).await)
}

async fn briefings(State(s): State<AppState>) -> Json<Value> {
    Json(json!(briefing::list(s.kernel.events(), 30)))
}

async fn briefing_now(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(briefing::write(&s, None).await.map_err(e400)?))
}

async fn backups(State(s): State<AppState>) -> Json<Value> {
    Json(json!({"dir": backup::backup_dir(&s).display().to_string(), "backups": backup::list(&s)}))
}

async fn backup_now(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(backup::run(&s).await.map_err(e400)?))
}

async fn gpu(State(s): State<AppState>) -> Json<Value> {
    let c = s.kernel.dirigent().conditions();
    let mut v = s.autonomy.gpu.status().await;
    v["conditions"] =
        json!({"user_idle": c.user_idle, "external_gpu_load": c.external_gpu_load, "pressure": c.pressure});
    Json(v)
}

async fn progress_view(State(s): State<AppState>) -> Json<Value> {
    Json(progress::compute(s.kernel.events()))
}

async fn overview(State(s): State<AppState>) -> Json<Value> {
    let q = &s.autonomy.queue;
    let c = s.kernel.dirigent().conditions();
    Json(json!({
        "user_idle": c.user_idle,
        "external_gpu_load": c.external_gpu_load,
        "running": q.list(Some("running"), None, 20),
        "queued": q.list(Some("queued"), None, 50).len(),
        "suggestions": monitor::list(s.kernel.events(), Some("new")).len(),
        "last_briefing": briefing::list(s.kernel.events(), 1).first().map(|b| b["day"].clone()),
        "remote": crate::autonomy::remote_info(&s),
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/autonomy", get(overview))
        .route("/api/autonomy/tasks", get(tasks).post(add_task))
        .route("/api/autonomy/tasks/:id", get(task))
        .route("/api/autonomy/tasks/:id/:action", post(task_action))
        .route("/api/autonomy/projects", get(projects).post(add_project))
        .route("/api/autonomy/projects/:id", get(project).post(update_project))
        .route("/api/autonomy/triggers", get(triggers).post(add_trigger))
        .route("/api/autonomy/triggers/:id", post(update_trigger))
        .route("/api/autonomy/suggestions", get(suggestions))
        .route("/api/autonomy/suggestions/scan", post(scan))
        .route("/api/autonomy/suggestions/:id/:action", post(decide))
        .route("/api/autonomy/briefings", get(briefings).post(briefing_now))
        .route("/api/autonomy/backups", get(backups).post(backup_now))
        .route("/api/autonomy/gpu", get(gpu))
        .route("/api/autonomy/progress", get(progress_view))
        .merge(super::openai::routes())
}
