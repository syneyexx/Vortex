//! Experimenten (W17): schaduw-runs van nieuwe configs (W17.03), A/B van prompts, modellen en
//! strategieën op de eval-suites met gepaarde statistiek (W17.05), metamorfe + counterfactual
//! tests (W17.04), positionele contexttests per model (W17.07) en ablations per component
//! (W17.02, 🔒 alleen op commando). Elk experiment = rij in `experiments` + event.
// VX:W17.03

use std::collections::HashMap;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_eval::{Runner, Suite};
use vortex_types::{Event, EventId, EventKind, TraceId};

use crate::api::{ApiError, AppState};
use crate::chat::{Mode, Variant};
use crate::eval_api::{VortexSubject, evals_dir, python};

fn start(state: &AppState, kind: &str, spec: &Value) -> Result<String, String> {
    let id = format!("exp_{}", EventId::new().to_string().trim_start_matches("evt_"));
    let now = vortex_types::now().to_rfc3339();
    state
        .kernel
        .events()
        .db()
        .write(|tx| {
            tx.execute("INSERT INTO experiments(id, kind, spec, status, created_at, updated_at) VALUES (?1,?2,?3,'running',?4,?4)", params![id, kind, spec.to_string(), now])?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

fn finish(state: &AppState, id: &str, r: &Result<Value, String>) {
    let (status, result) = match r {
        Ok(v) => ("done", v.clone()),
        Err(e) => ("failed", json!({"error": e})),
    };
    let log = state.kernel.events();
    let _ = log.db().write(|tx| {
        tx.execute(
            "UPDATE experiments SET status = ?2, result = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, status, result.to_string(), vortex_types::now().to_rfc3339()],
        )?;
        Ok(())
    });
    let _ = log.append(Event::new(EventKind::ExperimentCompleted, TraceId::new(), json!({"id": id, "status": status})));
}

fn suites(names: &[String]) -> Result<Vec<Suite>, String> {
    let all = vortex_eval::load_dir(&evals_dir().join("sets"))?;
    let v: Vec<Suite> =
        all.into_iter().filter(|s| names.is_empty() || names.iter().any(|n| n == "all" || *n == s.name)).collect();
    if v.is_empty() { Err("geen suite gevonden".into()) } else { Ok(v) }
}

async fn run_with(
    state: &AppState,
    s: &Suite,
    v: Option<Variant>,
    label: &str,
) -> Result<Vec<vortex_eval::TaskResult>, String> {
    let subject = VortexSubject { state: state.clone(), mode_override: None, variant: v };
    let runner = Runner { log: state.kernel.events(), subject: &subject, python: python() };
    let r = runner.run_suite(s, Some(label)).await?;
    vortex_eval::results(state.kernel.events(), &r.id)
}

/// W17.05: A/B op de eval-suites, gepaard per taak.
pub async fn ab(state: &AppState, names: &[String], a: Variant, b: Variant) -> Result<Value, String> {
    let mut pairs = Vec::new();
    let mut per_suite = Vec::new();
    for s in suites(names)? {
        let ra = run_with(state, &s, Some(a.clone()), &format!("A:{}", a.name)).await?;
        let rb = run_with(state, &s, Some(b.clone()), &format!("B:{}", b.name)).await?;
        let bm: HashMap<&str, f64> =
            rb.iter().filter(|r| r.status != "skip").map(|r| (r.task_id.as_str(), r.score)).collect();
        let sp: Vec<(String, f64, f64)> = ra
            .iter()
            .filter(|r| r.status != "skip")
            .filter_map(|r| bm.get(r.task_id.as_str()).map(|sb| (format!("{}:{}", s.name, r.task_id), r.score, *sb)))
            .collect();
        per_suite.push(json!({"suite": s.name, "stats": vortex_eval::stats::paired(&sp)}));
        pairs.extend(sp);
    }
    Ok(json!({"a": a, "b": b, "overall": vortex_eval::stats::paired(&pairs), "suites": per_suite}))
}

/// W17.03: schaduw — recente echte vragen opnieuw met de huidige én de kandidaat-config;
/// een blinde Judge vergelijkt (volgorde wisselt). Niets ervan bereikt de gebruiker.
pub async fn shadow(state: &AppState, candidate: Variant, n: usize) -> Result<Value, String> {
    let qs: Vec<String> = state
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT DISTINCT content FROM messages WHERE role = 'user' AND length(content) > 8 AND content NOT LIKE '/%' ORDER BY created_at DESC LIMIT ?1")?;
            let v = st.query_map([n as i64 * 2], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|q| vortex_cognition::route(q).task != vortex_cognition::TaskType::Personal)
        .take(n)
        .collect();
    if qs.is_empty() {
        return Err("nog geen vragen om in de schaduw te draaien".into());
    }
    let cfg = state.kernel.config().current();
    let judge = state
        .chat
        .role_llm(&cfg, "judge", state.kernel.shutdown_token().child_token())
        .or_else(|| state.chat.role_llm(&cfg, "chat", state.kernel.shutdown_token().child_token()))
        .ok_or("geen model voor de Judge")?;
    let (mut wins, mut losses, mut ties) = (0usize, 0usize, 0usize);
    let mut items = Vec::new();
    for (i, q) in qs.iter().enumerate() {
        let base = state.chat.answer_variant(q, Mode::Quick, None, None).await.map_err(|e| e.to_string())?;
        let cand =
            state.chat.answer_variant(q, Mode::Quick, None, Some(&candidate)).await.map_err(|e| e.to_string())?;
        let flip = i % 2 == 1;
        let (x, y) = if flip { (&cand, &base) } else { (&base, &cand) };
        let out = judge
            .complete(
                vec![
                    vortex_models::backend::Message::system("Vergelijk twee antwoorden op dezelfde vraag op juistheid, volledigheid en eerlijkheid. JSON: {\"winner\": \"1\"|\"2\"|\"gelijk\", \"why\": \"…\"}"),
                    vortex_models::backend::Message::user(format!("Vraag: {q}\n\nAntwoord 1:\n{x}\n\nAntwoord 2:\n{y}")),
                ],
                vortex_cognition::GenOpts { max_tokens: Some(200), temperature: Some(0.0), json_schema: None, reasoning_effort: None },
            )
            .await
            .unwrap_or_default();
        let v = vortex_agents::json::extract(&out).unwrap_or_default();
        let w = v["winner"].as_str().unwrap_or("gelijk");
        let cand_won = (w == "2" && !flip) || (w == "1" && flip);
        let base_won = (w == "1" && !flip) || (w == "2" && flip);
        if cand_won {
            wins += 1;
        } else if base_won {
            losses += 1;
        } else {
            ties += 1;
        }
        items.push(json!({"question": q, "winner": if cand_won { "kandidaat" } else if base_won { "huidig" } else { "gelijk" }, "why": v["why"]}));
    }
    let decided = wins + losses;
    let win_rate = if decided > 0 { wins as f64 / decided as f64 } else { 0.5 };
    Ok(
        json!({"candidate": candidate, "n": qs.len(), "wins": wins, "losses": losses, "ties": ties, "win_rate": win_rate, "advice": if win_rate > 0.6 && decided >= 5 { "kandidaat beter: overnemen" } else if win_rate < 0.4 && decided >= 5 { "kandidaat slechter: niet overnemen" } else { "geen duidelijk verschil: meer vragen nodig" }, "items": items}),
    )
}

/// W17.04: metamorf + counterfactual op een suite.
pub async fn metamorphic(state: &AppState, names: &[String]) -> Result<Value, String> {
    let mut out = Vec::new();
    for s in suites(names)? {
        let o = run_with(state, &s, None, "metamorf:origineel").await?;
        let d = vortex_eval::metamorphic::derive(&s);
        if d.tasks.is_empty() {
            continue;
        }
        let r = run_with(state, &d, None, "metamorf:varianten").await?;
        out.push(json!({"suite": s.name, "report": vortex_eval::metamorphic::report(&o, &r)}));
    }
    Ok(json!({"suites": out}))
}

/// W17.07: naald-in-hooiberg per model.
pub async fn positional(state: &AppState, model: Option<String>, lengths: Vec<usize>) -> Result<Value, String> {
    let s = vortex_eval::positional::suite(&lengths);
    let v = Variant {
        name: format!("positional:{}", model.clone().unwrap_or_else(|| "standaard".into())),
        model: model.clone(),
        mode: Some("quick".into()),
        ..Default::default()
    };
    let r = run_with(state, &s, Some(v), "positioneel").await?;
    Ok(json!({"model": model, "lengths": lengths, "report": vortex_eval::positional::report(&r)}))
}

/// W17.15: MPO (tensor-netwerk) vs int8 op een eigen neurale layer. Alleen layers uit de
/// spec-lijst (router, salience, …) — het redeneermodel is hier per ontwerp niet te kiezen.
pub async fn compression(state: &AppState, layer: &str, chis: Vec<u32>) -> Result<Value, String> {
    use vortex_neural::{dataset, spec};
    let sp = spec::get(layer).ok_or_else(|| format!("onbekende layer {layer} (alleen eigen neurale layers)"))?;
    let ex = crate::neural_api::build_examples(&state.chat, layer);
    if ex.len() < 40 {
        return Err(format!("te weinig voorbeelden voor {layer}: {}", ex.len()));
    }
    let dir = crate::neural_api::layers_dir(&state.chat).join("datasets");
    let meta = dataset::write(&dir, &sp, ex, &dataset::eval_index(&crate::neural_api::eval_prompts()))
        .map_err(|e| e.to_string())?;
    let classes = match sp.kind {
        spec::LayerKind::Multiclass(n) => n,
        spec::LayerKind::Binary => 1,
    };
    let out = crate::neural_api::layers_dir(&state.chat).join("tmp").join(format!("compression-{layer}.json"));
    let toml = format!(
        "train = {:?}\ntest = {:?}\nclasses = {classes}\nhidden = {}\nchis = [{}]\nresult = {:?}\n",
        meta.train_path.display().to_string(),
        meta.test_path.display().to_string(),
        sp.hidden.max(16),
        chis.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", "),
        out.display().to_string()
    );
    let (p, _) = state.chat.worker_job("experiment:compression", toml).await?;
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(json!({"layer": layer, "dataset": meta.version, "n": meta.n, "result": v}))
}

// VX:W17.02
/// W17.02 🔒: per component uit (ablation) vs alles aan, gepaard.
pub async fn ablation(state: &AppState, names: &[String], components: Vec<String>) -> Result<Value, String> {
    let mut rows = Vec::new();
    for c in components {
        let base = Variant { name: "volledig".into(), ..Default::default() };
        let off = Variant { name: format!("zonder {c}"), ablate: vec![c.clone()], ..Default::default() };
        let r = ab(state, names, base, off).await?;
        let o = &r["overall"];
        rows.push(json!({"component": c, "full": o["mean_a"], "without": o["mean_b"], "value": -o["diff"].as_f64().unwrap_or(0.0), "ci": [o["ci_low"], o["ci_high"]], "verdict": if o["verdict"] == "A" { "draagt bij" } else if o["verdict"] == "B" { "maakt het slechter: kandidaat om te schrappen" } else { "geen meetbaar effect: kandidaat om te vereenvoudigen" }}));
    }
    Ok(json!({"components": rows}))
}

fn e400(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}

pub(crate) fn spawn_exp<F>(state: &AppState, kind: &str, spec: Value, f: F) -> Result<Json<Value>, ApiError>
where
    F: std::future::Future<Output = Result<Value, String>> + Send + 'static,
{
    let id = start(state, kind, &spec).map_err(e400)?;
    let (st, id2) = (state.clone(), id.clone());
    tokio::spawn(async move {
        let r = f.await;
        finish(&st, &id2, &r);
    });
    Ok(Json(json!({"id": id, "status": "running"})))
}

#[derive(Debug, Deserialize)]
struct AbBody {
    #[serde(default)]
    suites: Vec<String>,
    a: Variant,
    b: Variant,
}

async fn ab_h(State(s): State<AppState>, Json(b): Json<AbBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    let spec = json!({"suites": b.suites, "a": b.a, "b": b.b});
    spawn_exp(&s, "ab", spec, async move { ab(&st, &b.suites, b.a, b.b).await })
}

#[derive(Debug, Deserialize)]
struct ShadowBody {
    candidate: Variant,
    #[serde(default = "ten")]
    n: usize,
}
fn ten() -> usize {
    10
}

async fn shadow_h(State(s): State<AppState>, Json(b): Json<ShadowBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    let spec = json!({"candidate": b.candidate, "n": b.n});
    spawn_exp(&s, "shadow", spec, async move { shadow(&st, b.candidate, b.n.clamp(1, 50)).await })
}

#[derive(Debug, Deserialize)]
struct SuitesBody {
    #[serde(default)]
    suites: Vec<String>,
}

async fn meta_h(State(s): State<AppState>, Json(b): Json<SuitesBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    spawn_exp(&s, "metamorphic", json!({"suites": b.suites}), async move { metamorphic(&st, &b.suites).await })
}

#[derive(Debug, Deserialize)]
struct PosBody {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    lengths: Vec<usize>,
}

async fn pos_h(State(s): State<AppState>, Json(b): Json<PosBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    let lengths = if b.lengths.is_empty() { vec![2000, 8000, 24000] } else { b.lengths };
    spawn_exp(&s, "positional", json!({"model": b.model, "lengths": lengths}), async move {
        positional(&st, b.model, lengths).await
    })
}

#[derive(Debug, Deserialize)]
struct AblBody {
    #[serde(default)]
    suites: Vec<String>,
    #[serde(default)]
    components: Vec<String>,
    /// 🔒 Grote test: alleen met expliciete bevestiging van de gebruiker.
    #[serde(default)]
    confirm: String,
}

async fn abl_h(State(s): State<AppState>, Json(b): Json<AblBody>) -> Result<Json<Value>, ApiError> {
    if b.confirm != "GROTE TESTEN NU" {
        return Err(e400("🔒 ablations zijn een grote test: alleen met confirm = \"GROTE TESTEN NU\""));
    }
    let comps = if b.components.is_empty() {
        crate::chat::ABLATABLE.iter().map(|x| x.to_string()).collect()
    } else {
        b.components
    };
    let st = s.clone();
    spawn_exp(&s, "ablation", json!({"suites": b.suites, "components": comps}), async move {
        ablation(&st, &b.suites, comps).await
    })
}

#[derive(Debug, Deserialize)]
struct CompBody {
    layer: String,
    #[serde(default)]
    chis: Vec<u32>,
}

async fn comp_h(State(s): State<AppState>, Json(b): Json<CompBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    let chis = if b.chis.is_empty() { vec![2, 4, 8] } else { b.chis };
    spawn_exp(&s, "compression", json!({"layer": b.layer, "chis": chis}), async move {
        compression(&st, &b.layer, chis).await
    })
}

#[derive(Debug, Deserialize)]
struct PromptOptBody {
    #[serde(default)]
    suites: Vec<String>,
    #[serde(default = "three")]
    k: usize,
}
fn three() -> usize {
    3
}

async fn prompt_opt_h(State(s): State<AppState>, Json(b): Json<PromptOptBody>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    spawn_exp(&s, "prompt_opt", json!({"suites": b.suites, "k": b.k}), async move {
        crate::selfimprove::optimize_prompt(&st, &b.suites, b.k.clamp(1, 5)).await
    })
}

async fn list(State(s): State<AppState>) -> Json<Value> {
    let v: Vec<Value> = s
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, kind, spec, status, result, created_at, updated_at FROM experiments ORDER BY created_at DESC LIMIT 100")?;
            let v = st
                .query_map([], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "spec": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or_default(), "status": r.get::<_, String>(3)?, "result": r.get::<_, Option<String>>(4)?.and_then(|x| serde_json::from_str::<Value>(&x).ok()), "created_at": r.get::<_, String>(5)?, "updated_at": r.get::<_, String>(6)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    Json(json!(v))
}

async fn one(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let Json(all) = list(State(s)).await;
    all.as_array()
        .and_then(|a| a.iter().find(|x| x["id"] == id.as_str()).cloned())
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onbekend experiment"))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/experiments", get(list))
        .route("/api/experiments/:id", get(one))
        .route("/api/experiments/ab", post(ab_h))
        .route("/api/experiments/shadow", post(shadow_h))
        .route("/api/experiments/metamorphic", post(meta_h))
        .route("/api/experiments/positional", post(pos_h))
        .route("/api/experiments/ablation", post(abl_h))
        .route("/api/experiments/compression", post(comp_h))
        .route("/api/experiments/prompt_opt", post(prompt_opt_h))
}
