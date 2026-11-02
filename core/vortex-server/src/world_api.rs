//! Wereldkennis-API (W14): bronnen (config/world.toml), planner met inhaal-modus (P3-runs via de
//! Dirigent), events, bias, hypotheses, voorspellingen + toetsen + kalibratie, samenvattingen,
//! spelers, gevolgde sites en HF-datasets (worker: inspect/import → bestemming + catalogus).
// VX:W14.02

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_agents::world::{self, WorldDeps};
use vortex_core::{EventLog, RunSpec};
use vortex_crawler::world::{SourceSpec, WorldMonitor, cluster::cluster, terms};
use vortex_proto::common::v1::TraceContext;
use vortex_proto::worker::v1::worker_service_client::WorkerServiceClient;
use vortex_proto::worker::v1::{TrainRequest, train_update::Update};
use vortex_types::{Event, EventKind, Priority, RunKind, TraceId};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

#[derive(Debug, Clone, Deserialize)]
pub struct WorldCfg {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "thr")]
    pub cluster_threshold: f64,
    #[serde(default = "win")]
    pub window_hours: i64,
    #[serde(default = "six")]
    pub hypothesize_every_hours: f64,
    #[serde(default = "six")]
    pub predict_every_hours: f64,
    #[serde(default = "thirty")]
    pub horizon_days: i64,
    #[serde(default)]
    pub sources: Vec<SourceSpec>,
}
fn yes() -> bool {
    true
}
fn thr() -> f64 {
    0.34
}
fn win() -> i64 {
    72
}
fn six() -> f64 {
    6.0
}
fn thirty() -> i64 {
    30
}

impl Default for WorldCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            cluster_threshold: thr(),
            window_hours: win(),
            hypothesize_every_hours: six(),
            predict_every_hours: six(),
            horizon_days: thirty(),
            sources: vec![],
        }
    }
}

#[derive(Debug)]
struct NoLlm;
#[async_trait::async_trait]
impl vortex_cognition::CogLlm for NoLlm {
    async fn complete(
        &self,
        _m: Vec<vortex_models::backend::Message>,
        _o: vortex_cognition::GenOpts,
    ) -> Result<String, String> {
        Err("geen model geconfigureerd".into())
    }
}

impl ChatEngine {
    pub fn world_cfg(&self) -> WorldCfg {
        self.kernel.config().current().values.get("world").and_then(|v| v.clone().try_into().ok()).unwrap_or_default()
    }

    pub(crate) fn world(&self) -> Arc<WorldMonitor> {
        let mut g = self.world.lock();
        if let Some(w) = g.as_ref() {
            return w.clone();
        }
        let w = Arc::new(WorldMonitor::new(self.kernel.events().clone()));
        *g = Some(w.clone());
        w
    }

    pub(crate) fn world_deps(&self) -> WorldDeps {
        let cfg = self.kernel.config().current();
        let llm =
            self.role_llm(&cfg, "chat", self.kernel.shutdown_token().child_token()).unwrap_or_else(|| Arc::new(NoLlm));
        WorldDeps { log: self.kernel.events().clone(), llm, memory: self.memory.as_ref().map(|m| m.store.clone()) }
    }
}

/// Laatste keer dat een periodieke stap liep (in `world_sources` met een `__`-naam, zodat ook
/// dit na een herstart doorloopt: inhaal-modus).
fn last_step(log: &EventLog, step: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    log.db()
        .read(|c| {
            Ok(c.query_row("SELECT last_ok FROM world_sources WHERE name = ?1", [step], |r| {
                r.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten())
        })
        .ok()
        .flatten()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
        .map(|t| t.with_timezone(&chrono::Utc))
}

fn mark_step(log: &EventLog, step: &str) {
    let now = vortex_types::now().to_rfc3339();
    let _ = log.db().write(|tx| {
        tx.execute(
            "INSERT INTO world_sources(name, last_fetch, last_ok) VALUES (?1, ?2, ?2) ON CONFLICT(name) DO UPDATE SET last_fetch = ?2, last_ok = ?2",
            params![step, now],
        )?;
        Ok(())
    });
}

fn step_due(log: &EventLog, step: &str, every_h: f64) -> bool {
    last_step(log, step).is_none_or(|t| (chrono::Utc::now() - t).num_minutes() as f64 >= every_h * 60.0)
}

/// Eén planner-cyclus: bronnen die aan de beurt zijn (met inhaal-venster), clusteren, gevolgde
/// sites, KG-integratie; en — alleen als de gebruiker niet actief is en er een model is —
/// hypotheses, voorspellingen, toetsen en samenvattingen. Draait als P3-run (Lopend werk).
pub async fn cycle(chat: &ChatEngine, force: Option<&str>) -> Result<Value, String> {
    let cfg = chat.world_cfg();
    let mon = chat.world();
    let log = chat.kernel.events().clone();
    let due: Vec<SourceSpec> = cfg
        .sources
        .iter()
        .filter(|s| s.enabled)
        .filter(|s| match force {
            Some("all") => true,
            Some(n) => s.name == n,
            None => mon.due(s),
        })
        .cloned()
        .collect();
    let llm_steps = force.is_none() && chat.kernel.dirigent().conditions().user_idle;
    let mut spec = RunSpec::new(RunKind::Other("world".into()), Priority::P3);
    spec.input = json!({"sources": due.iter().map(|s| s.name.clone()).collect::<Vec<_>>()});
    let ctx = chat.kernel.start_run(spec).map_err(|e| e.to_string())?;
    ctx.start().map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for s in &due {
        reports.push(mon.fetch(s).await);
    }
    let cl = cluster(&log, cfg.window_hours, cfg.cluster_threshold);
    let browser = chat.crawler().cfg.browser.clone();
    let changed = mon.recrawl_due(browser.as_deref()).await;
    // Wikidata vóór de KG-integratie, zodat entiteiten met Q-ID in het geheugen komen.
    let linked = match cfg.sources.iter().find(|s| s.kind == "wikidata" && s.enabled) {
        Some(w) => mon.link_entities(&w.url, w.params.get("lang").and_then(|v| v.as_str()).unwrap_or("en"), 20).await,
        None => 0,
    };
    let d = chat.world_deps();
    let integrated = world::integrate(&d, 50).await;
    let mut out = json!({"fetched": reports, "cluster": cl, "followed_changed": changed, "integrated": integrated, "wikidata_linked": linked});
    let has_model = chat.registry().is_ok();
    if has_model && (llm_steps || force == Some("all")) {
        if step_due(&log, "__resolve", 1.0) {
            out["resolved"] = json!(world::resolve_due(&d).await.len());
            mark_step(&log, "__resolve");
        }
        if step_due(&log, "__hypothesize", cfg.hypothesize_every_hours) {
            out["hypotheses"] = json!(world::hypothesize(&d, 48).await.len());
            mark_step(&log, "__hypothesize");
        }
        if step_due(&log, "__predict", cfg.predict_every_hours) {
            out["predictions"] = json!(world::predict(&d, cfg.horizon_days, 3).await.len());
            mark_step(&log, "__predict");
        }
        for (p, h) in [("day", 24.0), ("week", 24.0 * 7.0), ("month", 24.0 * 30.0)] {
            let step = format!("__summary_{p}");
            if step_due(&log, &step, h) {
                let n = world::summarize(&d, p, "theme").await.len() + world::summarize(&d, p, "region").await.len();
                out[format!("summaries_{p}")] = json!(n);
                mark_step(&log, &step);
            }
        }
    }
    let _ = ctx.complete(out.clone());
    Ok(out)
}

/// Planner: elke minuut kijken wat aan de beurt is; na een pauze haalt elke bron het gemiste
/// venster in (sinds de laatste geslaagde keer).
pub fn spawn_world(state: AppState, stop: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            let cfg = state.chat.world_cfg();
            if !cfg.enabled || cfg.sources.is_empty() {
                continue;
            }
            if let Err(e) = cycle(&state.chat, None).await {
                tracing::warn!(error = %e, "wereldmonitor-cyclus mislukt");
            }
        }
    });
}

fn e400(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}

fn rows(log: &EventLog, sql: &str) -> Vec<Value> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(sql)?;
            let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
            let v = st
                .query_map([], |r| {
                    let mut o = serde_json::Map::new();
                    for (i, n) in names.iter().enumerate() {
                        let val = match r.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(x) => json!(x),
                            rusqlite::types::ValueRef::Real(x) => json!(x),
                            rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                            rusqlite::types::ValueRef::Blob(_) => Value::Null,
                        };
                        o.insert(n.clone(), val);
                    }
                    Ok(Value::Object(o))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

async fn status(State(s): State<AppState>) -> Json<Value> {
    let cfg = s.chat.world_cfg();
    let log = s.kernel.events();
    let stats = rows(log, "SELECT name, last_fetch, last_ok, items, errors, last_error FROM world_sources");
    let sources: Vec<Value> = cfg
        .sources
        .iter()
        .map(|sp| {
            let st = stats.iter().find(|r| r["name"] == sp.name.as_str()).cloned().unwrap_or_default();
            let (t, gap) = terms(&sp.kind);
            json!({"name": sp.name, "kind": sp.kind, "url": sp.url, "enabled": sp.enabled, "interval_min": sp.interval_min, "country": sp.country, "orientation": sp.orientation, "terms": t, "min_gap_s": gap, "stats": st})
        })
        .collect();
    let counts = rows(
        log,
        "SELECT (SELECT COUNT(*) FROM world_items) AS items, (SELECT COUNT(*) FROM world_events) AS events, (SELECT COUNT(*) FROM world_events WHERE status = 'confirmed') AS confirmed, (SELECT COUNT(*) FROM predictions WHERE status = 'open') AS open_predictions, (SELECT COUNT(*) FROM hypotheses) AS hypotheses",
    );
    let steps = rows(log, "SELECT name, last_ok FROM world_sources WHERE name LIKE '\\_\\_%' ESCAPE '\\'");
    Json(
        json!({"enabled": cfg.enabled, "sources": sources, "counts": counts.first(), "steps": steps, "followed": s.chat.world().followed()}),
    )
}

#[derive(Debug, Deserialize)]
struct FetchBody {
    #[serde(default)]
    source: Option<String>,
}

async fn fetch_now(State(s): State<AppState>, Json(b): Json<FetchBody>) -> Result<Json<Value>, ApiError> {
    let f = b.source.unwrap_or_else(|| "all".into());
    Ok(Json(cycle(&s.chat, Some(&f)).await.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct EvQ {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    hours: Option<i64>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    region: Option<String>,
}

async fn events(State(s): State<AppState>, Query(q): Query<EvQ>) -> Json<Value> {
    let since = (vortex_types::now() - chrono::Duration::hours(q.hours.unwrap_or(72))).to_rfc3339();
    let v: Vec<_> = world::events(s.kernel.events(), &since, q.status.as_deref(), 300)
        .into_iter()
        .filter(|e| q.theme.as_ref().is_none_or(|t| e.theme.as_ref() == Some(t)))
        .filter(|e| q.region.as_ref().is_none_or(|t| e.region.as_ref() == Some(t)))
        .collect();
    Json(json!(v))
}

async fn event(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    world::event_detail(s.kernel.events(), &id)
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onbekend event"))
}

#[derive(Debug, Deserialize)]
struct ItemQ {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

async fn items(State(s): State<AppState>, Query(q): Query<ItemQ>) -> Json<Value> {
    let like = format!("%{}%", q.q.unwrap_or_default().to_lowercase());
    let src = q.source.unwrap_or_default();
    let v: Vec<Value> = s
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, source, kind, title, url, published, country, event_id FROM world_items WHERE lower(title || ' ' || summary) LIKE ?1 AND (?2 = '' OR source = ?2) ORDER BY COALESCE(published, fetched_at) DESC LIMIT 200")?;
            let v = st
                .query_map(params![like, src], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "source": r.get::<_, String>(1)?, "kind": r.get::<_, String>(2)?, "title": r.get::<_, String>(3)?, "url": r.get::<_, Option<String>>(4)?, "published": r.get::<_, Option<String>>(5)?, "country": r.get::<_, Option<String>>(6)?, "event_id": r.get::<_, Option<String>>(7)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    Json(json!(v))
}

async fn bias(State(s): State<AppState>) -> Json<Value> {
    let cfg = s.chat.world_cfg();
    let mut v = s.chat.world().source_bias();
    for b in &mut v {
        if let Some(sp) = cfg.sources.iter().find(|x| b["source"] == x.name.as_str()) {
            b["orientation"] = json!(sp.orientation);
            b["home"] = json!(sp.country);
        }
    }
    Json(json!(v))
}

async fn hypotheses(State(s): State<AppState>) -> Json<Value> {
    Json(json!(world::hypotheses(s.kernel.events(), 100)))
}

#[derive(Debug, Deserialize)]
struct PredQ {
    #[serde(default)]
    status: Option<String>,
}

async fn predictions(State(s): State<AppState>, Query(q): Query<PredQ>) -> Json<Value> {
    Json(json!(world::predictions(s.kernel.events(), q.status.as_deref(), 300)))
}

#[derive(Debug, Deserialize)]
struct NewPred {
    claim: String,
    probability: f64,
    criteria: String,
    resolve_by: String,
    #[serde(default)]
    actor: Option<String>,
}

async fn add_prediction(State(s): State<AppState>, Json(b): Json<NewPred>) -> Result<Json<Value>, ApiError> {
    let p = world::add_prediction(
        s.kernel.events(),
        &b.claim,
        b.probability,
        &b.criteria,
        &b.resolve_by,
        b.actor.as_deref(),
        "user",
    )
    .map_err(e400)?;
    Ok(Json(json!(p)))
}

async fn resolve(State(s): State<AppState>) -> Json<Value> {
    Json(json!(world::resolve_due(&s.chat.world_deps()).await))
}

async fn calibration(State(s): State<AppState>) -> Json<Value> {
    Json(world::calibration(s.kernel.events()))
}

#[derive(Debug, Deserialize)]
struct SumBody {
    #[serde(default = "day")]
    period: String,
    #[serde(default = "theme")]
    group_by: String,
}
fn day() -> String {
    "day".into()
}
fn theme() -> String {
    "theme".into()
}

async fn summaries(State(s): State<AppState>, Json(b): Json<SumBody>) -> Json<Value> {
    Json(json!(world::summarize(&s.chat.world_deps(), &b.period, &b.group_by).await))
}

async fn players(State(s): State<AppState>) -> Json<Value> {
    Json(json!(world::players(s.kernel.events())))
}

#[derive(Debug, Deserialize)]
struct NewPlayer {
    name: String,
    sector: String,
    kind: String,
    #[serde(default)]
    public_role: String,
}

async fn add_player(State(s): State<AppState>, Json(b): Json<NewPlayer>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(world::add_player(s.kernel.events(), &b.name, &b.sector, &b.kind, &b.public_role).map_err(e400)?)))
}

async fn study(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(world::study_player(&s.chat.world_deps(), &id).await.map_err(e400)?)))
}

#[derive(Debug, Deserialize)]
struct PlayerPred {
    situation: String,
    #[serde(default = "thirty")]
    horizon_days: i64,
}

async fn player_predict(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<PlayerPred>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(world::predict_player(&s.chat.world_deps(), &id, &b.situation, b.horizon_days).await.map_err(e400)?)))
}

#[derive(Debug, Deserialize)]
struct FollowBody {
    url: String,
    #[serde(default = "twentyfour")]
    interval_hours: f64,
    #[serde(default)]
    remove: bool,
}
fn twentyfour() -> f64 {
    24.0
}

async fn follow(State(s): State<AppState>, Json(b): Json<FollowBody>) -> Result<Json<Value>, ApiError> {
    let w = s.chat.world();
    if b.remove {
        return Ok(Json(json!({"removed": w.unfollow(&b.url)})));
    }
    if !b.url.starts_with("http") {
        return Err(e400("URL moet met http(s) beginnen"));
    }
    w.follow(&b.url, b.interval_hours).map_err(e400)?;
    Ok(Json(json!({"followed": w.followed()})))
}

// ── HF-datasets (W14.09) ──────────────────────────────────────────────────────

impl ChatEngine {
    fn hf_dir(&self) -> PathBuf {
        crate::research_api::cold_dir(&self.kernel.config().current(), &self.data_dir()).join("hf-imports")
    }

    pub(crate) async fn worker_job(
        &self,
        job: &str,
        toml: String,
    ) -> Result<(String, std::collections::HashMap<String, f64>), String> {
        let addr: String = self
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
        let ch =
            ep.connect().await.map_err(|_| format!("Python-worker niet bereikbaar op {addr} (start: just worker)"))?;
        let mut client = WorkerServiceClient::new(ch);
        let req = TrainRequest {
            trace: Some(TraceContext { trace_id: TraceId::new().to_string(), ..Default::default() }),
            job_kind: job.into(),
            config_toml: toml,
            dataset: None,
        };
        let mut stream = client.train(req).await.map_err(|e| e.message().to_string())?.into_inner();
        let (mut path, mut metrics) = (None, std::collections::HashMap::new());
        while let Some(u) = stream.message().await.map_err(|e| e.message().to_string())? {
            metrics.extend(u.metrics.clone());
            match u.update {
                Some(Update::Artifact(a)) => path = Some(a.path),
                Some(Update::Error(e)) => return Err(e),
                _ => {}
            }
        }
        path.map(|p| (p, metrics)).ok_or_else(|| "worker gaf geen resultaat".into())
    }
}

pub(crate) fn toml_kv(v: &Value) -> String {
    v.as_object()
        .map(|o| {
            o.iter()
                .filter(|(_, x)| !x.is_null() && !x.is_object())
                .map(|(k, x)| {
                    format!(
                        "{k} = {}\n",
                        if x.is_string() {
                            toml::Value::String(x.as_str().unwrap_or("").to_string()).to_string()
                        } else {
                            x.to_string()
                        }
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct Inspect {
    source: String,
    #[serde(default)]
    split: Option<String>,
    #[serde(default)]
    config: Option<String>,
    #[serde(default)]
    license: Option<String>,
}

/// Schema + mapping-voorstel. Met een model verfijnt het LM het voorstel; de gebruiker bevestigt.
async fn hf_inspect(State(s): State<AppState>, Json(b): Json<Inspect>) -> Result<Json<Value>, ApiError> {
    let out = s.chat.hf_dir().join(format!("inspect-{}.json", vortex_tools::stable_hash(b.source.as_bytes())));
    let toml = toml_kv(
        &json!({"source": b.source, "split": b.split, "config": b.config, "license": b.license, "out": out.display().to_string()}),
    );
    let (p, _) = s.chat.worker_job("dataset:inspect", toml).await.map_err(e400)?;
    let mut info: Value = serde_json::from_str(&std::fs::read_to_string(&p).map_err(e400)?).map_err(e400)?;
    if s.chat.registry().is_ok() {
        let d = s.chat.world_deps();
        let v = d
            .llm
            .complete(
                vec![
                    vortex_models::backend::Message::system("Stel een veldmapping voor van deze dataset naar Vortex-velden (prompt, response, messages, chosen, rejected, text, title, label, code). Alleen bestaande veldnamen. JSON: {\"mapping\": {\"doelveld\": \"bronveld\"}, \"kind\": \"chat|qa|preference|code|text\", \"why\": \"…\"}"),
                    vortex_models::backend::Message::user(format!("Schema: {}\nVoorbeeld: {}\nHeuristisch voorstel: {}", info["schema"], info["sample"], info["mapping"])),
                ],
                vortex_cognition::GenOpts { max_tokens: Some(300), temperature: Some(0.0), json_schema: None, reasoning_effort: None },
            )
            .await
            .ok()
            .and_then(|o| vortex_agents::json::extract(&o));
        if let Some(v) = v {
            let fields = info["schema"]["fields"].as_object().cloned().unwrap_or_default();
            if let Some(m) = v["mapping"].as_object()
                && m.values().all(|x| x.as_str().is_some_and(|f| fields.contains_key(f)))
            {
                info["lm_mapping"] = json!(m);
                info["lm_kind"] = v["kind"].clone();
                info["lm_why"] = v["why"].clone();
            }
        }
    }
    Ok(Json(info))
}

#[derive(Debug, Deserialize)]
struct Import {
    source: String,
    destination: String,
    /// Door de gebruiker bevestigde mapping (verplicht: geen import zonder bevestiging).
    mapping: serde_json::Map<String, Value>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    split: Option<String>,
    #[serde(default)]
    config: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    name: Option<String>,
}

async fn hf_import(State(s): State<AppState>, Json(b): Json<Import>) -> Result<Json<Value>, ApiError> {
    Ok(Json(import_dataset(&s.chat, b).await.map_err(e400)?))
}

async fn import_dataset(chat: &ChatEngine, b: Import) -> Result<Value, String> {
    if b.mapping.is_empty() {
        return Err("bevestig eerst de mapping".into());
    }
    let name: String = b
        .name
        .clone()
        .unwrap_or_else(|| b.source.rsplit(['/', '\\']).next().unwrap_or("dataset").to_string())
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let dir = chat.hf_dir();
    let out = dir.join(format!("{name}-{}.jsonl", b.destination));
    let existing: Vec<String> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|x| x == "jsonl")
                        && p != &out
                        && p.to_string_lossy().contains(&format!("-{}.", b.destination))
                })
                .map(|p| p.display().to_string())
                .collect()
        })
        .unwrap_or_default();
    let mapping_toml: String =
        b.mapping.iter().map(|(k, v)| format!("{k} = {:?}\n", v.as_str().unwrap_or(""))).collect();
    let toml = format!(
        "{}existing = {:?}\n[mapping]\n{mapping_toml}",
        toml_kv(
            &json!({"source": b.source, "destination": b.destination, "out": out.display().to_string(), "kind": b.kind, "split": b.split, "config": b.config, "license": b.license, "limit": b.limit.unwrap_or(10_000), "evals_dir": crate::eval_api::evals_dir().join("sets").display().to_string()})
        ),
        existing
    );
    let mut spec = RunSpec::new(RunKind::Ingest, Priority::P3);
    spec.input = json!({"hf": b.source, "destination": b.destination});
    let ctx = chat.kernel.start_run(spec).map_err(|e| e.to_string())?;
    ctx.start().map_err(|e| e.to_string())?;
    let r = chat.worker_job("dataset:import", toml).await;
    let (meta_path, _) = match r {
        Ok(x) => x,
        Err(e) => {
            let _ = ctx.fail(&e);
            return Err(e);
        }
    };
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let rows: Vec<Value> = std::fs::read_to_string(&out)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    // Naar de bestemming.
    let dest_info = match b.destination.as_str() {
        "knowledge" => {
            let mem = chat.memory.as_ref().ok_or("geen geheugen")?;
            let mut n = 0;
            for r in &rows {
                let mut m = vortex_memory::NewMemory::new(
                    vortex_memory::MemoryKind::Semantic,
                    r["content"].as_str().unwrap_or(""),
                    vortex_types::Provenance::with_origin(vortex_types::SourceKind::Web, format!("hf:{}", b.source)),
                );
                m.title = r["title"].as_str().unwrap_or("").chars().take(120).collect();
                m.source_uri = Some(format!("hf:{}", b.source));
                m.source_hash = Some(format!("hf:{}:{}", b.source, r["row"]));
                m.locator = r["locator"].as_str().map(String::from);
                m.tags = vec!["hf".into(), format!("dataset:{name}")];
                m.force = true;
                if mem.store.write_or_defer(m).await.is_ok() {
                    n += 1;
                }
            }
            json!({"memories": n})
        }
        "sft" | "dpo" => {
            let kind = b.destination.as_str();
            let text = |r: &Value| {
                if kind == "sft" {
                    r["messages"].to_string()
                } else {
                    format!("{} {} {}", r["prompt"], r["chosen"], r["rejected"])
                }
            };
            crate::training_api::save_dataset(
                chat,
                kind,
                rows.clone(),
                text,
                json!({"hf": b.source, "license": meta["license"]}),
            )?
        }
        "eval" => {
            let p = crate::eval_api::evals_dir().join("sets").join(format!("hf_{name}.jsonl"));
            let body = format!(
                "// Vortex eval-set uit HF-dataset {} (licentie: {}). Gegenereerd door W14.09.\n{}",
                b.source,
                meta["license"].as_str().unwrap_or("onbekend"),
                rows.iter().map(|r| format!("{r}\n")).collect::<String>()
            );
            std::fs::write(&p, body).map_err(|e| e.to_string())?;
            json!({"eval_set": p.display().to_string()})
        }
        "layer" => {
            let p = crate::neural_api::layers_dir(chat).join("hf");
            std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
            let f = p.join(format!("{name}.jsonl"));
            std::fs::copy(&out, &f).map_err(|e| e.to_string())?;
            json!({"layer_data": f.display().to_string()})
        }
        d => return Err(format!("onbekende bestemming {d}")),
    };
    let id = format!("hf_{name}_{}", meta["version"].as_str().unwrap_or(""));
    let now = vortex_types::now().to_rfc3339();
    let log = chat.kernel.events();
    let _ = log.db().write(|tx| {
        tx.execute(
            "INSERT OR REPLACE INTO hf_catalog(id, source, schema, mapping, destination, license, rows, kept, deduped, decontaminated, status, output, version, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'imported',?11,?12,?13)",
            params![
                id,
                b.source,
                meta["kind"].as_str().unwrap_or(""),
                Value::Object(b.mapping.clone()).to_string(),
                b.destination,
                meta["license"].as_str(),
                meta["rows"].as_i64().unwrap_or(0),
                meta["kept"].as_i64().unwrap_or(0),
                meta["exact_dup"].as_i64().unwrap_or(0) + meta["near_dup"].as_i64().unwrap_or(0),
                meta["decontaminated"].as_i64().unwrap_or(0),
                out.display().to_string(),
                meta["version"].as_str().unwrap_or(""),
                now
            ],
        )?;
        Ok(())
    });
    let res = json!({"id": id, "stats": meta, "destination": dest_info});
    let _ = log.append(Event::new(EventKind::DatasetImported, ctx.trace().trace_id, res.clone()));
    let _ = ctx.complete(res.clone());
    Ok(res)
}

async fn hf_catalog(State(s): State<AppState>) -> Json<Value> {
    Json(json!(rows(s.kernel.events(), "SELECT * FROM hf_catalog ORDER BY created_at DESC")))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/world", get(status))
        .route("/api/world/fetch", post(fetch_now))
        .route("/api/world/events", get(events))
        .route("/api/world/events/:id", get(event))
        .route("/api/world/items", get(items))
        .route("/api/world/bias", get(bias))
        .route("/api/world/hypotheses", get(hypotheses))
        .route("/api/world/predictions", get(predictions).post(add_prediction))
        .route("/api/world/predictions/resolve", post(resolve))
        .route("/api/world/calibration", get(calibration))
        .route("/api/world/summaries", post(summaries))
        .route("/api/world/players", get(players).post(add_player))
        .route("/api/world/players/:id/study", post(study))
        .route("/api/world/players/:id/predict", post(player_predict))
        .route("/api/world/follow", post(follow))
        .route("/api/world/datasets", get(hf_catalog))
        .route("/api/world/datasets/inspect", post(hf_inspect))
        .route("/api/world/datasets/import", post(hf_import))
}
