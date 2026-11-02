//! Lokale OpenAI-compatibele API (W15.07) voor Cursor en eigen projecten.
//! - Model `vortex` / `vortex-quick` / `vortex-deep` / `vortex-research`: volledige Vortex-pijplijn
//!   (geheugen, cognitie, debatteam, tools) in een eigen sessie per gesprek van de client.
//! - Elk ander model uit de registry: doorgeven aan de llama-server (incl. tools en streaming).
// VX:W15.07

use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use vortex_types::{EventKind, SessionId};

use crate::api::AppState;
use crate::chat::{Mode, SendRequest};

const VORTEX_MODELS: &[(&str, &str)] =
    &[("vortex", "adaptive"), ("vortex-quick", "quick"), ("vortex-deep", "deep"), ("vortex-research", "research")];

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({"error": {"message": msg, "type": "invalid_request_error"}}))).into_response()
}

async fn models(State(s): State<AppState>) -> Json<Value> {
    let created = 1_700_000_000;
    let mut data: Vec<Value> = VORTEX_MODELS
        .iter()
        .map(|(m, _)| json!({"id": m, "object": "model", "created": created, "owned_by": "vortex"}))
        .collect();
    if let Ok(reg) = s.chat.registry() {
        for (name, e) in &reg.models {
            if e.roles.iter().any(|r| r == "chat") {
                data.push(json!({"id": name, "object": "model", "created": created, "owned_by": "local"}));
            }
        }
    }
    Json(json!({"object": "list", "data": data}))
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Sessie per gesprek: sleutel = client-gebruiker + eerste gebruikersbericht.
fn session_for(s: &AppState, user: &str, first: &str) -> Result<SessionId, String> {
    let key = format!("{user}|{:016x}", vortex_tools::stable_hash(first.as_bytes()));
    let log = s.kernel.events();
    let found: Option<String> = log
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT session_id FROM api_sessions WHERE key = ?1", [&key], |r| r.get(0)).optional()?)
        })
        .map_err(|e| e.to_string())?;
    if let Some(id) = found.and_then(|x| x.parse::<SessionId>().ok())
        && s.chat.sessions().get(id).is_ok()
    {
        return Ok(id);
    }
    let sess = s.chat.new_session().map_err(|e| e.to_string())?;
    let _ = s.chat.sessions().rename(sess.id, &format!("API · {}", first.chars().take(40).collect::<String>()));
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO api_sessions(key, session_id, created_at) VALUES (?1, ?2, ?3)",
                params![key, sess.id.to_string(), vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(sess.id)
}

fn chunk(id: &str, model: &str, delta: Value, finish: Option<&str>) -> String {
    let c = json!({"id": id, "object": "chat.completion.chunk", "created": chrono::Utc::now().timestamp(), "model": model, "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    format!("data: {c}\n\n")
}

async fn completions(State(s): State<AppState>, Json(b): Json<Value>) -> Response {
    let model = b["model"].as_str().unwrap_or("vortex").to_string();
    let stream = b["stream"].as_bool().unwrap_or(false);
    let Some(mode) = VORTEX_MODELS.iter().find(|(m, _)| *m == model).map(|x| x.1) else {
        return passthrough(&s, &model, b).await;
    };
    let msgs = b["messages"].as_array().cloned().unwrap_or_default();
    let users: Vec<String> = msgs.iter().filter(|m| m["role"] == "user").map(|m| text_of(&m["content"])).collect();
    let (Some(first), Some(last)) = (users.first(), users.last()) else {
        return err(StatusCode::BAD_REQUEST, "geen gebruikersbericht");
    };
    let sid = match session_for(&s, b["user"].as_str().unwrap_or("api"), first) {
        Ok(x) => x,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    // Eerst luisteren, dan versturen (geen deltas missen).
    let mut rx = s.kernel.events().subscribe();
    let reply = match s.chat.send(sid, SendRequest { text: last.clone(), mode: Mode::parse(mode), model: None }) {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let run = reply.run_id;
    let id = format!("chatcmpl-{}", run.to_string().trim_start_matches("run_"));
    let (tx, out) = tokio::sync::mpsc::channel::<Result<String, std::convert::Infallible>>(64);
    let (id2, model2) = (id.clone(), model.clone());
    let log = s.kernel.events().clone();
    tokio::spawn(async move {
        let mut full = String::new();
        let mut status = "complete".to_string();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
        if stream {
            let _ = tx.send(Ok(chunk(&id2, &model2, json!({"role": "assistant", "content": ""}), None))).await;
        }
        loop {
            let ev = tokio::select! {
                e = rx.recv() => e,
                _ = tokio::time::sleep_until(deadline) => { status = "timeout".into(); break; }
            };
            let ev = match ev {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Achterstand: het antwoord staat volledig in het bericht; daar lezen we straks.
                    continue;
                }
                Err(_) => break,
            };
            if ev.run_id != Some(run) {
                continue;
            }
            match ev.kind {
                EventKind::MessageDelta => {
                    let t = ev.payload["text"].as_str().unwrap_or("");
                    if !t.is_empty() {
                        full.push_str(t);
                        if stream && tx.send(Ok(chunk(&id2, &model2, json!({"content": t}), None))).await.is_err() {
                            return; // client weg; de run loopt door in Vortex
                        }
                    }
                }
                EventKind::MessageCompleted => {
                    status = ev.payload["status"].as_str().unwrap_or("complete").to_string();
                    break;
                }
                _ => {}
            }
        }
        let _ = &log;
        let finish = if status == "complete" { "stop" } else { "length" };
        if stream {
            let _ = tx.send(Ok(chunk(&id2, &model2, json!({}), Some(finish)))).await;
            let _ = tx.send(Ok("data: [DONE]\n\n".into())).await;
        } else {
            let body = json!({
                "id": id2, "object": "chat.completion", "created": chrono::Utc::now().timestamp(), "model": model2,
                "choices": [{"index": 0, "message": {"role": "assistant", "content": full}, "finish_reason": finish}],
                "usage": {"prompt_tokens": 0, "completion_tokens": full.len() / 4, "total_tokens": full.len() / 4},
                "vortex": {"status": status}
            });
            let _ = tx.send(Ok(body.to_string())).await;
        }
    });
    let body_stream = futures::stream::unfold(out, |mut rx| async move { rx.recv().await.map(|x| (x, rx)) });
    let ct = if stream { "text/event-stream" } else { "application/json" };
    Response::builder()
        .header(header::CONTENT_TYPE, ct)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(body_stream))
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "respons"))
}

/// Ander model: rechtstreeks naar de llama-server van dat model (beheerde server eerst laden).
async fn passthrough(s: &AppState, model: &str, b: Value) -> Response {
    let Ok(reg) = s.chat.registry() else { return err(StatusCode::INTERNAL_SERVER_ERROR, "geen registry") };
    let Some(entry) = reg.models.get(model) else {
        return err(StatusCode::NOT_FOUND, &format!("onbekend model {model}"));
    };
    let Some(server) = reg.server(&entry.gpu) else { return err(StatusCode::NOT_FOUND, "server onbekend") };
    s.autonomy.gpu.ensure(&reg, model).await;
    let url = format!("{}/v1/chat/completions", server.url.trim_end_matches('/'));
    let r = match reqwest::Client::new().post(&url).json(&b).send().await {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, &e.to_string()),
    };
    let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let ct = r
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from_stream(r.bytes_stream()))
        .unwrap_or_else(|_| err(StatusCode::BAD_GATEWAY, "respons"))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/v1/models", get(models)).route("/v1/chat/completions", post(completions))
}
