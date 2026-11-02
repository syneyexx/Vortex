//! Frontier (W18): elke component staat standaard UIT en gaat pas aan na meetbare winst
//! (gate: "elk onderdeel levert meetbare eval-winst, anders niet ingeschakeld"). De meting zit
//! per component in een experiment; `config/frontier.toml` zet de vlaggen.
// VX:W18.01

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FrontierCfg {
    /// W18.01: associatief geheugen als extra reranker (experiment `assoc`).
    pub assoc: bool,
    /// W18.02: replay van de vorige dataset bij het hertrainen van layers + vergeet-check.
    pub continual_replay: bool,
    /// W18.02: L2-SP naar de vorige gewichten (+ fine-tunen vanaf de vorige versie).
    pub continual_l2sp: bool,
    /// W18.03: causale claims uit antwoorden bijhouden.
    pub causal_tracking: bool,
    /// W18.07: privé + gedeeld geheugen voor het debatteam.
    pub agent_memory: bool,
}

impl crate::chat::ChatEngine {
    pub fn frontier(&self) -> FrontierCfg {
        self.kernel
            .config()
            .current()
            .values
            .get("frontier")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default()
    }
}

// ── W18.01: associatief geheugen ─────────────────────────────────────────────

use std::sync::Arc;

use serde_json::{Value, json};
use vortex_memory::assoc::{AssocMemory, AssocReranker};

use crate::api::AppState;

impl crate::chat::ChatEngine {
    pub(crate) fn assoc_path(&self) -> std::path::PathBuf {
        self.data_dir().join("frontier").join("assoc.json")
    }

    /// Alleen als aangezet én getraind (na gemeten winst).
    pub(crate) fn assoc_reranker(&self) -> Option<Arc<dyn vortex_memory::Reranker>> {
        if !self.frontier().assoc {
            return None;
        }
        let mem = self.memory.as_ref()?;
        let m: AssocMemory = serde_json::from_str(&std::fs::read_to_string(self.assoc_path()).ok()?).ok()?;
        Some(Arc::new(AssocReranker { mem: Arc::new(parking_lot::RwLock::new(m)), embedder: mem.store.embedder() }))
    }
}

/// Experiment: train op de helft van de retrieval-set, meet MRR op de andere helft (zonder vs
/// met associatief geheugen). Bewaart het getrainde geheugen; aanzetten = `frontier.assoc`.
pub async fn assoc_experiment(state: &AppState) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(crate::eval_api::evals_dir().join("retrieval.json")).map_err(|e| e.to_string())?;
    let set: vortex_eval::RetrievalSet = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let emb = state.memory.store.embedder();
    let docs: std::collections::HashMap<String, String> =
        set.corpus.iter().map(|d| (d.id.clone(), d.text.clone())).collect();
    let pool: Vec<String> = set.corpus.iter().map(|d| d.text.clone()).collect();
    let (mut train, mut test) = (Vec::new(), set.clone());
    test.questions.clear();
    for (i, q) in set.questions.iter().enumerate() {
        if i % 2 == 0 && !q.no_answer {
            for r in &q.relevant {
                if let Some(t) = docs.get(r) {
                    train.push((q.q.clone(), t.clone()));
                }
            }
        } else {
            test.questions.push(q.clone());
        }
    }
    let rr =
        AssocReranker { mem: Arc::new(parking_lot::RwLock::new(AssocMemory::new(emb.dims()))), embedder: emb.clone() };
    let loss = rr.fit(&train, &pool, 30).await.map_err(|e| e.to_string())?;
    let llm: Arc<dyn vortex_memory::MemLlm> = Arc::new(vortex_memory::HeuristicLlm);
    let base = vortex_eval::retrieval::run(&test, emb.clone(), llm.clone(), None).await?;
    let with =
        vortex_eval::retrieval::run(&test, emb, llm, Some(Arc::new(rr.clone()) as Arc<dyn vortex_memory::Reranker>))
            .await?;
    let p = state.chat.assoc_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(&p, serde_json::to_string(&*rr.mem.read()).unwrap_or_default()).map_err(|e| e.to_string())?;
    let gain = with.mrr - base.mrr;
    Ok(
        json!({"train_pairs": train.len(), "eval_questions": test.questions.len(), "loss": loss, "mrr_zonder": base.mrr, "mrr_met": with.mrr, "winst": gain, "advies": if gain > 0.02 { "winst gemeten: zet frontier.assoc = true" } else { "geen meetbare winst: uit laten (gate W18)" }}),
    )
}

// ── W18.03/04: causaal redeneren + simulaties over de hypotheses (W14) ───────

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize as De;

use crate::api::ApiError;

/// Causale graaf uit de hypotheses (oorzaak → gevolg met vertrouwen).
pub fn graph(log: &vortex_core::EventLog) -> vortex_cognition::causal::CausalGraph {
    let mut g = vortex_cognition::causal::CausalGraph::default();
    let rows: Vec<(String, String, f64)> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT cause, effect, confidence FROM hypotheses WHERE contra < support")?;
            let v =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    for (c, e, p) in rows {
        g.add(&c, &e, p);
    }
    g
}

/// W18.03 tracking: causale claims uit een antwoord → hypotheses (steun +1).
pub fn track_claims(log: &vortex_core::EventLog, text: &str) -> usize {
    let claims = vortex_cognition::causal::extract_claims(text);
    let now = vortex_types::now().to_rfc3339();
    let _ = log.db().write(|tx| {
        for (c, e) in &claims {
            tx.execute(
                "INSERT INTO hypotheses(id, cause, effect, confidence, support, contra, last_seen) VALUES (?1, ?2, ?3, 0.5, 1, 0, ?4)
                 ON CONFLICT(cause, effect) DO UPDATE SET support = support + 1, last_seen = ?4",
                rusqlite::params![format!("hy_t_{:016x}", vortex_tools::stable_hash(format!("{c}|{e}").as_bytes())), c, e, now],
            )?;
        }
        Ok(())
    });
    claims.len()
}

#[derive(Debug, De)]
struct EffectsBody {
    cause: String,
    #[serde(default)]
    evidence: Vec<String>,
}

async fn effects(State(s): State<AppState>, Json(b): Json<EffectsBody>) -> Json<Value> {
    Json(json!({"cause": b.cause, "effects": graph(s.kernel.events()).effects_of(&b.cause, &b.evidence)}))
}

async fn counterfactual(State(s): State<AppState>, Json(b): Json<EffectsBody>) -> Json<Value> {
    let v: Vec<Value> = graph(s.kernel.events())
        .counterfactual(&b.cause, &b.evidence)
        .into_iter()
        .map(|(n, a, z)| json!({"effect": n, "with": a, "without": z, "difference": a - z}))
        .collect();
    Json(json!({"without": b.cause, "effects": v}))
}

#[derive(Debug, De)]
struct SimBody {
    #[serde(default)]
    on: Vec<String>,
    #[serde(default)]
    off: Vec<String>,
    #[serde(default)]
    runs: Option<usize>,
}

async fn simulate(State(s): State<AppState>, Json(b): Json<SimBody>) -> Json<Value> {
    Json(json!(graph(s.kernel.events()).simulate(&b.on, &b.off, b.runs.unwrap_or(2000).clamp(10, 100_000), 42)))
}

async fn assoc_h(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let st = s.clone();
    crate::experiments::spawn_exp(&s, "assoc", json!({}), async move { assoc_experiment(&st).await })
}

async fn status(State(s): State<AppState>) -> Json<Value> {
    Json(json!({"flags": s.chat.frontier(), "assoc_trained": s.chat.assoc_path().exists()}))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/frontier", axum::routing::get(status))
        .route("/api/causal/effects", post(effects))
        .route("/api/causal/counterfactual", post(counterfactual))
        .route("/api/world/simulate", post(simulate))
        .route("/api/experiments/assoc", post(assoc_h))
}
