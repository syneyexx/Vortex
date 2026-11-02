//! Observability voor de GUI: feedback, trace per run, session replay, analytics. VX:W06.05
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_types::{Event, EventId, EventKind, MessageId, RunId, SessionId, TraceId};

use crate::api::{ApiError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/messages/:id/feedback", post(feedback))
        .route("/api/runs/:id/trace", get(trace))
        .route("/api/sessions/:id/replay", get(replay))
        .route("/api/analytics", get(analytics))
}

fn bad(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}
fn e500(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Debug, Deserialize)]
struct FeedbackBody {
    rating: String,
    #[serde(default)]
    correction: Option<String>,
}

/// Feedback-knoppen (goed / fout / correctie) → tabel + event (trainingsdata). VX:W06.07
async fn feedback(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<FeedbackBody>,
) -> Result<Json<Value>, ApiError> {
    if !matches!(b.rating.as_str(), "good" | "bad" | "correction") {
        return Err(bad("rating moet good/bad/correction zijn"));
    }
    let mid: MessageId = id.parse().map_err(bad)?;
    let msg = s.chat.sessions().message(mid).map_err(e500)?;
    let fid = EventId::new().to_string();
    s.kernel
        .events()
        .transact(|tx, out| {
            tx.execute(
                "INSERT INTO feedback(id, message_id, run_id, rating, correction, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![fid, mid.to_string(), msg.run_id.map(|r| r.to_string()), b.rating, b.correction, vortex_types::now().to_rfc3339()],
            )?;
            let mut e = Event::new(
                EventKind::FeedbackGiven,
                TraceId::new(),
                json!({"message_id": mid, "rating": b.rating, "correction": b.correction, "model": msg.model, "config_version": msg.config_version}),
            );
            e.run_id = msg.run_id;
            out.emit(tx, e)?;
            Ok(())
        })
        .map_err(e500)?;
    // Correctie = gebruiker heeft gelijk → als gebruikersfeit in het geheugen (hoog belang).
    if let (Some(c), "correction") = (&b.correction, b.rating.as_str()) {
        let mut m = vortex_memory::NewMemory::new(
            vortex_memory::MemoryKind::User,
            c.clone(),
            vortex_types::Provenance::with_origin(vortex_types::SourceKind::User, "correctie"),
        );
        m.importance = Some(0.9);
        m.force = true;
        let _ = s.memory.store.write_or_defer(m).await;
    }
    Ok(Json(json!({"ok": true})))
}

/// Trace van een run: alle events, spans afgeleid (wachten vs uitvoeren), inference-metingen.
async fn trace(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let run: RunId = id.parse().map_err(bad)?;
    let rec = s.kernel.runs().get(run).map_err(e500)?;
    let events = s.kernel.events().for_run(run).map_err(e500)?;
    let t0 = events.first().map(|e| e.ts).unwrap_or(rec.created_at);
    let ms = |t: vortex_types::Timestamp| (t - t0).num_milliseconds();
    let find = |k: EventKind| events.iter().find(|e| e.kind == k).map(|e| ms(e.ts));
    let queued = find(EventKind::TaskQueued);
    let admitted = find(EventKind::TaskAdmitted);
    let measured: Vec<&Value> =
        events.iter().filter(|e| e.kind == EventKind::InferenceMeasured).map(|e| &e.payload).collect();
    let end = events.last().map(|e| ms(e.ts)).unwrap_or(0);
    Ok(Json(json!({
        "run": rec,
        "spans": {
            "wait_ms": match (queued, admitted) { (Some(q), Some(a)) => Some(a - q), _ => None },
            "execute_ms": admitted.map(|a| end - a),
            "total_ms": end,
        },
        "inference": measured,
        "events": events.iter().map(|e| json!({"seq": e.seq, "t_ms": ms(e.ts), "kind": e.kind, "trace_id": e.trace_id, "span_id": e.span_id, "payload": e.payload})).collect::<Vec<_>>(),
    })))
}

/// Session replay: alle events van alle runs van een sessie, op volgorde. VX:W06.08
async fn replay(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let sid: SessionId = id.parse().map_err(bad)?;
    let msgs = s.chat.sessions().messages(sid).map_err(e500)?;
    let mut runs: Vec<RunId> = msgs.iter().filter_map(|m| m.run_id).collect();
    runs.dedup();
    let mut events = Vec::new();
    for r in runs {
        events.extend(s.kernel.events().for_run(r).map_err(e500)?);
    }
    events.sort_by_key(|e| e.seq);
    Ok(Json(json!({"session": sid, "events": events})))
}

/// Foutanalyse + tool/geheugen/model-analytics. VX:W06.09
async fn analytics(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let v = s
        .kernel
        .db()
        .read(|c| {
            let q = |sql: &str| -> rusqlite::Result<Vec<(String, i64)>> {
                let mut st = c.prepare(sql)?;
                let rows = st.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_else(|| "?".into()), r.get(1)?)))?;
                rows.collect()
            };
            let runs_by_state = q("SELECT state, COUNT(*) FROM runs GROUP BY state")?;
            let errors = q("SELECT substr(error, 1, 80), COUNT(*) FROM runs WHERE state = 'failed' GROUP BY substr(error, 1, 80) ORDER BY 2 DESC LIMIT 10")?;
            let tools = q("SELECT json_extract(payload, '$.tool'), COUNT(*) FROM events WHERE kind = 'tool.call.completed' GROUP BY 1 ORDER BY 2 DESC LIMIT 20")?;
            let tool_errors = q("SELECT json_extract(payload, '$.tool'), COUNT(*) FROM events WHERE kind IN ('tool.call.failed','tool.call.invalid') GROUP BY 1")?;
            let feedback = q("SELECT rating, COUNT(*) FROM feedback GROUP BY rating")?;
            let retrievals: i64 = c.query_row("SELECT COUNT(*) FROM events WHERE kind = 'memory.retrieval.performed'", [], |r| r.get(0))?;
            let avg_items: Option<f64> = c.query_row(
                "SELECT AVG(json_array_length(json_extract(payload, '$.items'))) FROM events WHERE kind = 'memory.retrieval.performed'",
                [],
                |r| r.get(0),
            )?;
            let (tps, ttft): (Option<f64>, Option<f64>) = c.query_row(
                "SELECT AVG(json_extract(payload, '$.decode_tps')), AVG(json_extract(payload, '$.ttft_ms')) FROM events WHERE kind = 'model.inference.measured'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let stalls: i64 = c.query_row("SELECT COUNT(*) FROM events WHERE kind = 'runtime.stall.detected'", [], |r| r.get(0))?;
            Ok(json!({
                "runs_by_state": runs_by_state, "top_errors": errors, "tools": tools, "tool_errors": tool_errors,
                "feedback": feedback, "memory": {"retrievals": retrievals, "avg_items": avg_items},
                "model": {"avg_decode_tps": tps, "avg_ttft_ms": ttft}, "stalls": stalls,
            }))
        })
        .map_err(e500)?;
    Ok(Json(v))
}
