//! Kwaliteitsdiagnostiek (W17.08): antwoord met logprobs → onzekere stukken, perplexiteit.
// VX:W17.08

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_models::backend::{Backend, ChatRequest, Message};

use crate::api::{ApiError, AppState};

#[derive(Debug, Deserialize)]
struct Diag {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default = "low")]
    low: f32,
}
fn low() -> f32 {
    0.35
}

async fn logprobs(State(s): State<AppState>, Json(b): Json<Diag>) -> Result<Json<Value>, ApiError> {
    let reg = s.chat.registry().map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let model = b.model.unwrap_or_else(|| reg.default.clone());
    let backend =
        s.chat.backends.for_model(&reg, &model).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut req = ChatRequest::new(vec![Message::user(b.prompt)]);
    req.logprobs = true;
    req.top_logprobs = Some(3);
    req.max_tokens = Some(512);
    let r = backend
        .chat(&req, &s.kernel.shutdown_token().child_token())
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let d = vortex_models::diagnostics::diagnose(&r.logprobs, b.low);
    Ok(Json(json!({"model": model, "answer": r.content, "diagnosis": d, "tokens": r.logprobs.len()})))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/diagnostics/logprobs", post(logprobs))
}
