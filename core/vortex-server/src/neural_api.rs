//! Neurale layers in de server (W11): registry koppelen aan geheugen/skills/router/salience,
//! datasets uit het event log + geheugen (W11.01), LM-bootstrap-labels (W11.02), training via de
//! worker (W11.03), evaluatie net vs heuristiek + automatische promotie/rollback (W11.07),
//! idle-training (W11.08), reranker-dataset (W11.06) en de dashboard-API (W11.09).
// VX:W11.01

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_cognition::GenOpts;
use vortex_core::{EventLog, RunSpec};
use vortex_memory::Filters;
use vortex_models::backend::Message;
use vortex_neural::dataset::{self, Example};
use vortex_neural::registry::VersionInfo;
use vortex_neural::{LayerKind, LayerRegistry, Mode, spec};
use vortex_proto::common::v1::TraceContext;
use vortex_proto::worker::v1::worker_service_client::WorkerServiceClient;
use vortex_proto::worker::v1::{TrainRequest, train_update::Update};
use vortex_types::{Event, EventId, EventKind, Priority, RunKind, TraceId};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

/// Minimaal aantal gelabelde voorbeelden om te trainen.
pub const MIN_EXAMPLES: usize = 40;

pub fn layers_dir(chat: &ChatEngine) -> PathBuf {
    chat.data_dir().join("layers")
}

/// Registry openen en aan alle consumenten koppelen.
pub fn attach(chat: &mut ChatEngine) -> Arc<LayerRegistry> {
    let reg = LayerRegistry::open(layers_dir(chat), Some(chat.kernel.events().clone()));
    let s: vortex_core::Scorer = Some(reg.clone());
    if let Some(m) = &chat.memory {
        m.store.set_scorer(s.clone());
    }
    if let Some(t) = &chat.tools {
        t.skills.set_scorer(s.clone());
    }
    chat.scorer = s;
    chat.layers = Some(reg.clone());
    reg
}

fn eval_prompts() -> Vec<String> {
    let dir = crate::eval_api::evals_dir().join("sets");
    vortex_eval::load_dir(&dir)
        .map(|s| s.into_iter().flat_map(|x| x.tasks.into_iter().map(|t| t.prompt)).collect())
        .unwrap_or_default()
}

fn examples_from_log(log: &EventLog, layer: &str) -> Vec<(Vec<f32>, Vec<f32>, String)> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT payload FROM events WHERE kind = 'neural.example.logged' AND json_extract(payload, '$.layer') = ?1 ORDER BY seq")?;
            let v = st
                .query_map([layer], |r| r.get::<_, String>(0))?
                .filter_map(|p| p.ok())
                .filter_map(|p| serde_json::from_str::<Value>(&p).ok())
                .map(|v| {
                    let f = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|y| y as f32)).collect::<Vec<_>>()).unwrap_or_default();
                    (f("x"), f("h"), v["ref"].as_str().unwrap_or("").to_string())
                })
                .collect::<Vec<_>>();
            Ok(v)
        })
        .unwrap_or_default()
}

fn labels(log: &EventLog, layer: &str) -> HashMap<String, (f32, String)> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT ref, label, source FROM neural_labels WHERE layer = ?1")?;
            let v = st
                .query_map([layer], |r| {
                    Ok((r.get::<_, String>(0)?, (r.get::<_, f64>(1)? as f32, r.get::<_, String>(2)?)))
                })?
                .collect::<rusqlite::Result<HashMap<_, _>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

pub fn set_label(log: &EventLog, layer: &str, reference: &str, label: f32, source: &str) -> Result<(), String> {
    let now = vortex_types::now().to_rfc3339();
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO neural_labels(layer, ref, label, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(layer, ref) DO UPDATE SET label = ?3, source = ?4, created_at = ?5",
                params![layer, reference, f64::from(label), source, now],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())
}

/// Feedback per run (duim omhoog = 1, omlaag = 0).
fn feedback(log: &EventLog) -> HashMap<String, f32> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT run_id, json_extract(payload, '$.rating') FROM events WHERE kind = 'feedback.user.given' AND run_id IS NOT NULL")?;
            let v = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?
                .filter_map(|x| x.ok())
                .filter_map(|(run, rating)| match rating.as_deref() {
                    Some("up") => Some((run, 1.0)),
                    Some("down") => Some((run, 0.0)),
                    _ => None,
                })
                .collect::<HashMap<_, _>>();
            Ok(v)
        })
        .unwrap_or_default()
}

/// Laatste keer dat een herinnering opgehaald werd (uit retrieval-events).
fn retrieved_at(log: &EventLog) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let rows: Vec<(String, String)> = log
        .db()
        .read(|c| {
            let mut st =
                c.prepare("SELECT ts, payload FROM events WHERE kind = 'memory.retrieval.performed' ORDER BY seq")?;
            let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    for (ts, p) in rows {
        if let Ok(v) = serde_json::from_str::<Value>(&p) {
            for it in v["items"].as_array().into_iter().flatten() {
                if let Some(id) = it["id"].as_str() {
                    out.insert(id.to_string(), ts.clone());
                }
            }
        }
    }
    out
}

/// Vraagtekst van een run (voor bootstrap-labels en decontaminatie).
fn run_question(chat: &ChatEngine, run: &str) -> Option<String> {
    chat.kernel
        .events()
        .db()
        .read(|c| {
            Ok(c.query_row(
                "SELECT m.content FROM messages m WHERE m.run_id = ?1 AND m.role = 'user' LIMIT 1",
                [run],
                |r| r.get::<_, String>(0),
            )
            .ok())
        })
        .ok()
        .flatten()
}

/// Voorbeelden + labels voor één layer (W11.01).
pub fn build_examples(chat: &ChatEngine, layer: &str) -> Vec<Example> {
    let log = chat.kernel.events();
    let lab = labels(log, layer);
    let mut out = Vec::new();
    match layer {
        "importance" | "decay" => {
            let Some(mem) = &chat.memory else { return out };
            let seen = retrieved_at(log);
            let now = chrono::Utc::now();
            let all = mem
                .store
                .list(&Filters { include_stale: true, ..Default::default() }, None, 20_000, 0)
                .unwrap_or_default();
            for m in all {
                let age_days = chrono::DateTime::parse_from_rfc3339(&m.created_at)
                    .map(|t| (now - t.with_timezone(&chrono::Utc)).num_hours() as f64 / 24.0)
                    .unwrap_or(0.0);
                let id = m.id.to_string();
                if layer == "importance" {
                    if age_days < 2.0 && !lab.contains_key(&id) {
                        continue; // uitkomst nog niet bekend
                    }
                    let (imp, f) = vortex_memory::policy::importance(&m.content, m.kind);
                    let y = lab.get(&id).map(|l| l.0).unwrap_or(if seen.contains_key(&id) { 1.0 } else { 0.0 });
                    out.push(Example {
                        x: vortex_memory::features::importance_vector(&f, m.kind),
                        y,
                        h: vec![imp as f32],
                        w: 1.0,
                        src: id,
                        label_source: "outcome".into(),
                        text: Some(m.content.chars().take(500).collect()),
                    });
                } else {
                    if age_days < 30.0 && !lab.contains_key(&id) {
                        continue;
                    }
                    let then = now - chrono::Duration::days(30);
                    let used = seen
                        .get(&id)
                        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                        .is_some_and(|t| t.with_timezone(&chrono::Utc) >= then);
                    let h = if m.access_count > 0 || m.importance >= 0.6 { 1.0 } else { 0.0 };
                    let y = lab.get(&id).map(|l| l.0).unwrap_or(if used { 1.0 } else { 0.0 });
                    out.push(Example {
                        x: vortex_memory::features::decay_vector(&m, then),
                        y,
                        h: vec![h],
                        w: 1.0,
                        src: id,
                        label_source: "outcome".into(),
                        text: None,
                    });
                }
            }
        }
        _ => {
            let fb = feedback(log);
            for (x, h, r) in examples_from_log(log, layer) {
                let label = match lab.get(&r) {
                    Some((y, s)) => Some((*y, s.clone())),
                    None if layer == "usefulness" => {
                        r.split(':').next().and_then(|run| fb.get(run)).map(|y| (*y, "outcome".to_string()))
                    }
                    None => None,
                };
                let Some((y, src)) = label else { continue };
                let run = r.split([':', '|']).next().unwrap_or("").to_string();
                let text = if ["router", "salience"].contains(&layer) { run_question(chat, &r) } else { None };
                out.push(Example {
                    x,
                    y,
                    h,
                    w: 1.0,
                    src: if run.is_empty() { r } else { run },
                    label_source: src,
                    text,
                });
            }
        }
    }
    out
}

async fn ask_json(llm: &Arc<dyn vortex_cognition::CogLlm>, sys: &str, user: String) -> Value {
    let out = llm
        .complete(
            vec![Message::system(sys.to_string()), Message::user(user)],
            GenOpts { max_tokens: Some(60), temperature: Some(0.0), json_schema: None, reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    vortex_agents::json::extract(&out).unwrap_or_default()
}

/// Bootstrap-labeling met het LM (W11.02) voor layers zonder directe uitkomst.
pub async fn bootstrap(
    chat: &ChatEngine,
    layer: &str,
    max: usize,
    llm: Arc<dyn vortex_cognition::CogLlm>,
) -> Result<usize, String> {
    let log = chat.kernel.events().clone();
    let have = labels(&log, layer);
    let mut done = 0;
    let mut seen = HashSet::new();
    for (_, _, r) in examples_from_log(&log, layer) {
        if done >= max || have.contains_key(&r) || !seen.insert(r.clone()) {
            continue;
        }
        let label: Option<f32> = match layer {
            "router" => {
                let Some(q) = run_question(chat, &r) else { continue };
                let v = ask_json(&llm, "Classificeer het verzoek in precies één taaktype: chat, code, math, analysis, research, planning, personal, factual. JSON: {\"task\": \"…\"}.", q).await;
                v["task"].as_str().and_then(|t| spec::ROUTER_CLASSES.iter().position(|c| *c == t)).map(|i| i as f32)
            }
            "salience" => {
                let Some(q) = run_question(chat, &r) else { continue };
                let v = ask_json(&llm, "Is dit bericht belangrijk of urgent voor de gebruiker (iets om te onthouden of snel op te volgen)? JSON: {\"important\": true|false}.", q).await;
                v["important"].as_bool().map(|b| if b { 1.0 } else { 0.0 })
            }
            "entity" => {
                let Some((a, b)) = r.split_once('|') else { continue };
                let v = ask_json(&llm, "Verwijzen deze twee namen naar hetzelfde ding (persoon, plek, project, concept)? JSON: {\"same\": true|false}.", format!("A: {a}\nB: {b}")).await;
                v["same"].as_bool().map(|b| if b { 1.0 } else { 0.0 })
            }
            "skill" | "usefulness" => {
                let Some((q, what)) = r.split_once('|').or_else(|| r.split_once(':')) else { continue };
                let v = ask_json(
                    &llm,
                    "Helpt dit bij het beantwoorden van de vraag? JSON: {\"helps\": true|false}.",
                    format!("Vraag/context: {q}\nKandidaat: {what}"),
                )
                .await;
                v["helps"].as_bool().map(|b| if b { 1.0 } else { 0.0 })
            }
            _ => None,
        };
        if let Some(l) = label {
            set_label(&log, layer, &r, l, "lm")?;
            done += 1;
        }
    }
    Ok(done)
}

/// Train via de worker (gRPC Train, `layer:<naam>`). Geeft het ONNX-pad en metrics.
async fn worker_train(
    chat: &ChatEngine,
    layer: &str,
    meta: &dataset::DatasetMeta,
    out: &std::path::Path,
    trace: TraceId,
) -> Result<(PathBuf, HashMap<String, f64>), String> {
    let addr: String = chat
        .kernel
        .config()
        .current()
        .get::<String>("paths.worker_address")
        .ok()
        .flatten()
        .unwrap_or_else(|| "127.0.0.1:50061".into());
    let ep = tonic::transport::Endpoint::from_shared(format!("http://{addr}"))
        .map_err(|e| e.to_string())?
        .connect_timeout(Duration::from_secs(2));
    let ch = ep.connect().await.map_err(|_| format!("Python-worker niet bereikbaar op {addr} (start: just worker)"))?;
    let sp = spec::get(layer).ok_or("onbekende layer")?;
    let classes = match sp.kind {
        LayerKind::Multiclass(n) => n,
        LayerKind::Binary => 1,
    };
    let toml = format!(
        "train = {:?}\ntest = {:?}\nout = {:?}\nclasses = {classes}\nhidden = {}\n",
        meta.train_path.display().to_string(),
        meta.test_path.display().to_string(),
        out.display().to_string(),
        sp.hidden
    );
    let mut client = WorkerServiceClient::new(ch);
    let req = TrainRequest {
        trace: Some(TraceContext { trace_id: trace.to_string(), ..Default::default() }),
        job_kind: format!("layer:{layer}"),
        config_toml: toml,
        dataset: None,
    };
    let mut stream = client.train(req).await.map_err(|e| e.message().to_string())?.into_inner();
    let (mut path, mut metrics) = (None, HashMap::new());
    while let Some(u) = stream.message().await.map_err(|e| e.message().to_string())? {
        for (k, v) in &u.metrics {
            metrics.insert(k.clone(), *v);
        }
        match u.update {
            Some(Update::Artifact(a)) => path = Some(PathBuf::from(a.path)),
            Some(Update::Error(e)) => return Err(e),
            _ => {}
        }
    }
    path.map(|p| (p, metrics)).ok_or_else(|| "worker gaf geen model terug".into())
}

/// Volledige cyclus voor één layer: dataset → train → installeren (schaduw) → eval → beslissen.
pub async fn train_layer(chat: &ChatEngine, layer: &str, priority: Priority) -> Result<Value, String> {
    let reg = chat.layers.clone().ok_or("layers niet actief")?;
    let sp = spec::get(layer).ok_or("onbekende layer")?;
    let mut rs = RunSpec::new(RunKind::Training, priority);
    rs.input = json!({"layer": layer});
    let ctx = chat.kernel.start_run(rs).map_err(|e| e.to_string())?;
    ctx.start().map_err(|e| e.to_string())?;
    let r = async {
        let ex = build_examples(chat, layer);
        if ex.len() < MIN_EXAMPLES {
            return Err(format!("te weinig gelabelde voorbeelden voor {layer}: {} (minimaal {MIN_EXAMPLES})", ex.len()));
        }
        let dir = layers_dir(chat).join("datasets");
        let meta = dataset::write(&dir, &sp, ex, &dataset::eval_index(&eval_prompts())).map_err(|e| e.to_string())?;
        let _ = chat.kernel.events().append(Event::new(EventKind::NeuralDataset, ctx.trace().trace_id, serde_json::to_value(&meta).unwrap_or_default()).for_run(ctx.id()));
        let tmp = layers_dir(chat).join("tmp").join(format!("{layer}-{}.onnx", meta.version));
        let (onnx, metrics) = worker_train(chat, layer, &meta, &tmp, ctx.trace().trace_id).await?;
        let version = format!("{}-{}", meta.version.trim_start_matches("ds-"), chrono::Utc::now().format("%m%d%H%M%S"));
        let info = VersionInfo { version: version.clone(), created_at: vortex_types::now().to_rfc3339(), dataset: meta.version.clone(), eval: None, train_metrics: metrics.clone().into_iter().collect() };
        reg.install(layer, &onnx, info)?;
        // Eval (schaduw + huidige actieve) op dezelfde testset → automatisch promoveren/terugdraaien.
        let test = dataset::read(&meta.test_path);
        let shadow_net = vortex_neural::Net::load(&layers_dir(chat).join(layer).join(format!("{version}.onnx")))?;
        let se = dataset::evaluate(&shadow_net, sp.kind, &test);
        reg.record_eval(layer, &version, se.clone())?;
        let ae = reg.manifest(layer).active.and_then(|a| vortex_neural::Net::load(&layers_dir(chat).join(layer).join(format!("{a}.onnx"))).ok()).map(|n| dataset::evaluate(&n, sp.kind, &test));
        let decision = reg.auto_decide(layer, Some(&se), ae.as_ref(), 0.02)?;
        let now = vortex_types::now().to_rfc3339();
        let _ = chat.kernel.events().db().write(|tx| {
            tx.execute(
                "INSERT INTO neural_runs(id, layer, dataset, version, n_train, n_test, net, heuristic, decision, trainer, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![EventId::new().to_string(), layer, meta.version, version, meta.n_train as i64, meta.n_test as i64, se.net, se.heuristic, decision, metrics.get("trainer").map(|v| v.to_string()), now],
            )?;
            Ok(())
        });
        Ok(json!({"layer": layer, "version": version, "dataset": meta, "eval": se, "active_eval": ae, "decision": decision, "metrics": metrics}))
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

/// Idle-training (W11.08): als de gebruiker niets doet, layers met genoeg nieuwe voorbeelden
/// opnieuw trainen (P3, via de Dirigent).
pub fn spawn_idle_training(chat: ChatEngine, stop: CancellationToken) {
    tokio::spawn(async move {
        let mut last: HashMap<String, usize> = HashMap::new();
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(600)) => {}
            }
            if !crate::eval_api::FEATURES.contains(&"W11") || !chat.kernel.dirigent().conditions().user_idle {
                continue;
            }
            for s in spec::specs() {
                let n = build_examples(&chat, s.name).len();
                if n >= MIN_EXAMPLES && n >= last.get(s.name).copied().unwrap_or(0) + 50 {
                    match train_layer(&chat, s.name, Priority::P3).await {
                        Ok(v) => tracing::info!(layer = s.name, decision = %v["decision"], "idle-training"),
                        Err(e) => tracing::debug!(layer = s.name, error = %e, "idle-training overgeslagen"),
                    }
                    last.insert(s.name.to_string(), n);
                }
            }
        }
    });
}

/// Reranker-dataset (W11.06): (vraag, passage, label) uit retrieval + feedback; fine-tunen gebeurt
/// met `worker/scripts/train_reranker.py` (PyTorch/transformers, op de PC).
pub fn reranker_dataset(chat: &ChatEngine) -> Result<(PathBuf, usize), String> {
    let log = chat.kernel.events();
    let fb = feedback(log);
    let mem = chat.memory.as_ref().ok_or("geen geheugen")?;
    let rows: Vec<(String, String)> = log
        .db()
        .read(|c| {
            let mut st = c.prepare(
                "SELECT run_id, payload FROM events WHERE kind = 'memory.retrieval.performed' AND run_id IS NOT NULL",
            )?;
            let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut n = 0;
    for (run, p) in rows {
        let Some(y) = fb.get(&run) else { continue };
        let Some(q) = run_question(chat, &run) else { continue };
        let v: Value = serde_json::from_str(&p).unwrap_or_default();
        for it in v["items"].as_array().into_iter().flatten().take(5) {
            let Some(m) = it["id"].as_str().and_then(|i| i.parse().ok()).and_then(|i| mem.store.get(i).ok()) else {
                continue;
            };
            out.push_str(
                &json!({"query": q, "passage": m.content.chars().take(1500).collect::<String>(), "label": y})
                    .to_string(),
            );
            out.push('\n');
            n += 1;
        }
    }
    let dir = layers_dir(chat).join("reranker");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("pairs.jsonl");
    std::fs::write(&p, out).map_err(|e| e.to_string())?;
    Ok((p, n))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/neural", get(status))
        .route("/api/neural/reranker/dataset", post(rr_dataset))
        .route("/api/neural/:layer/train", post(train))
        .route("/api/neural/:layer/bootstrap", post(boot))
        .route("/api/neural/:layer/promote", post(promote))
        .route("/api/neural/:layer/rollback", post(rollback))
        .route("/api/neural/:layer/mode", post(mode))
        .route("/api/neural/:layer/label", post(label))
}

fn reg(s: &AppState) -> Result<Arc<LayerRegistry>, ApiError> {
    s.chat.layers.clone().ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "layers niet actief"))
}

fn e400(e: String) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e)
}

async fn status(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let r = reg(&s)?;
    let runs: Vec<Value> = s
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT layer, version, n_train, n_test, net, heuristic, decision, created_at FROM neural_runs ORDER BY created_at DESC LIMIT 100")?;
            let v = st
                .query_map([], |r| {
                    Ok(json!({"layer": r.get::<_, String>(0)?, "version": r.get::<_, Option<String>>(1)?, "n_train": r.get::<_, i64>(2)?, "n_test": r.get::<_, i64>(3)?, "net": r.get::<_, Option<f64>>(4)?, "heuristic": r.get::<_, Option<f64>>(5)?, "decision": r.get::<_, Option<String>>(6)?, "created_at": r.get::<_, String>(7)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    let counts: HashMap<String, usize> =
        spec::specs().iter().map(|sp| (sp.name.to_string(), build_examples(&s.chat, sp.name).len())).collect();
    Ok(Json(
        json!({"specs": spec::specs(), "layers": r.status(), "runs": runs, "examples": counts, "min_examples": MIN_EXAMPLES}),
    ))
}

async fn train(State(s): State<AppState>, AxPath(layer): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(train_layer(&s.chat, &layer, Priority::P2).await.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct BootBody {
    #[serde(default)]
    max: Option<usize>,
}

async fn boot(
    State(s): State<AppState>,
    AxPath(layer): AxPath<String>,
    Json(b): Json<BootBody>,
) -> Result<Json<Value>, ApiError> {
    let cfg = s.kernel.config().current();
    let tok = s.kernel.shutdown_token().child_token();
    let llm = s
        .chat
        .role_llm(&cfg, "extract", tok.clone())
        .or_else(|| s.chat.role_llm(&cfg, "chat", tok))
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "geen model"))?;
    let n = bootstrap(&s.chat, &layer, b.max.unwrap_or(200), llm).await.map_err(e400)?;
    Ok(Json(json!({"labeled": n})))
}

async fn promote(State(s): State<AppState>, AxPath(layer): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"active": reg(&s)?.promote(&layer).map_err(e400)?})))
}

async fn rollback(State(s): State<AppState>, AxPath(layer): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"active": reg(&s)?.rollback(&layer, "handmatig").map_err(e400)?})))
}

#[derive(Debug, Deserialize)]
struct ModeBody {
    mode: Mode,
}

async fn mode(
    State(s): State<AppState>,
    AxPath(layer): AxPath<String>,
    Json(b): Json<ModeBody>,
) -> Result<Json<Value>, ApiError> {
    reg(&s)?.set_mode(&layer, b.mode).map_err(e400)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
struct LabelBody {
    reference: String,
    label: f32,
}

async fn label(
    State(s): State<AppState>,
    AxPath(layer): AxPath<String>,
    Json(b): Json<LabelBody>,
) -> Result<Json<Value>, ApiError> {
    set_label(s.kernel.events(), &layer, &b.reference, b.label, "human").map_err(e400)?;
    Ok(Json(json!({"ok": true})))
}

async fn rr_dataset(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let (p, n) = reranker_dataset(&s.chat).map_err(e400)?;
    Ok(Json(
        json!({"path": p, "pairs": n, "train": "cd worker && uv run --extra train python scripts/train_reranker.py --pairs <path>"}),
    ))
}
