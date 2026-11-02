//! Retrieval-eval apart van de rest (W06.03): recall@k, MRR, irrelevante passages, en
//! "geen-antwoord"-vragen. Draait op een geïsoleerd geheugen (in-memory database).
// VX:W06.03

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use vortex_core::{Db, EventLog, db::CORE_MIGRATIONS};
use vortex_memory::{Embedder, MemConfig, MemLlm, MemoryKind, MemoryStore, NewMemory, Query, Reranker, WriteOutcome};
use vortex_types::{Provenance, SourceKind};

use crate::model::RetrievalSet;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievalReport {
    pub questions: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    pub mrr: f64,
    /// Aandeel niet-relevante items in de top-5 (lager = beter).
    pub irrelevant_at_5: f64,
    /// Geen-antwoord-vragen waarbij niets "zeker" werd gevonden.
    pub no_answer_correct: f64,
    pub per_question: Vec<(String, Option<usize>)>,
}

/// Vanaf deze eindscore telt een treffer als "zeker gevonden" (voor geen-antwoord-vragen).
pub const CONFIDENT: f64 = 0.55;

pub async fn run(
    set: &RetrievalSet,
    embedder: Arc<dyn Embedder>,
    llm: Arc<dyn MemLlm>,
    reranker: Option<Arc<dyn Reranker>>,
) -> Result<RetrievalReport, String> {
    let db = Db::open_in_memory(CORE_MIGRATIONS).map_err(|e| e.to_string())?;
    let dims = embedder.dims();
    let store = MemoryStore::open(EventLog::new(Arc::new(db)), embedder, llm, MemConfig { dims, ..Default::default() })
        .map_err(|e| e.to_string())?;
    let mut by_mem: HashMap<String, String> = HashMap::new();
    for d in &set.corpus {
        let mut m =
            NewMemory::new(MemoryKind::Semantic, d.text.clone(), Provenance::with_origin(SourceKind::Dataset, "eval"));
        m.force = true;
        m.source_uri = Some(format!("doc:{}", d.id));
        match store.write(m).await.map_err(|e| e.to_string())? {
            WriteOutcome::Stored { ids, .. } => {
                for i in ids {
                    by_mem.insert(i.to_string(), d.id.clone());
                }
            }
            WriteOutcome::Merged { into } => {
                by_mem.entry(into.to_string()).or_insert(d.id.clone());
            }
            WriteOutcome::Skipped { .. } => {}
        }
    }
    let mut rep = RetrievalReport::default();
    let (mut r1, mut r3, mut r5, mut mrr, mut irr, mut answerable, mut na_ok, mut na_n) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0usize, 0usize, 0usize);
    for q in &set.questions {
        let mut query = Query::new(q.q.clone());
        query.k = 10;
        query.counter_evidence = false;
        let hits = store.retrieve(&query, reranker.as_ref()).await.map_err(|e| e.to_string())?;
        let docs: Vec<String> =
            hits.iter().map(|h| by_mem.get(&h.memory.id.to_string()).cloned().unwrap_or_default()).collect();
        if q.no_answer {
            na_n += 1;
            let confident =
                hits.first().is_some_and(|h| h.score.final_score >= CONFIDENT && h.score.vector.unwrap_or(0.0) >= 0.5);
            if !confident {
                na_ok += 1;
            }
            rep.per_question.push((q.q.clone(), None));
            continue;
        }
        answerable += 1;
        let rank = docs.iter().position(|d| q.relevant.contains(d)).map(|p| p + 1);
        if let Some(r) = rank {
            if r <= 1 {
                r1 += 1.0;
            }
            if r <= 3 {
                r3 += 1.0;
            }
            if r <= 5 {
                r5 += 1.0;
            }
            mrr += 1.0 / r as f64;
        }
        let top5: Vec<&String> = docs.iter().take(5).collect();
        if !top5.is_empty() {
            irr += top5.iter().filter(|d| !q.relevant.contains(d)).count() as f64 / top5.len() as f64;
        }
        rep.per_question.push((q.q.clone(), rank));
    }
    let a = answerable.max(1) as f64;
    rep.questions = set.questions.len();
    rep.recall_at_1 = r1 / a;
    rep.recall_at_3 = r3 / a;
    rep.recall_at_5 = r5 / a;
    rep.mrr = mrr / a;
    rep.irrelevant_at_5 = irr / a;
    rep.no_answer_correct = if na_n == 0 { 1.0 } else { na_ok as f64 / na_n as f64 };
    Ok(rep)
}
