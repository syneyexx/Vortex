//! Hybride ophalen (W05.07): query herschrijven → vector + BM25 (FTS5) + graph (multi-hop) +
//! temporeel → samenvoegen (RRF) → rerank → ranking (relevantie + recency + belang) → MMR →
//! tegenbewijs-slot. Elke score houdt zijn herkomst bij ("waarom opgehaald?").
// VX:W05.07

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Duration as CDuration, Utc};
use serde::Serialize;
use vortex_types::{Event, EventKind, MemoryId, TraceId};

use crate::error::Result;
use crate::store::{MemoryStore, mem_select, row_mem};
use crate::types::*;
use crate::vector::normalize;

/// Optionele reranker (Qwen3-Reranker op de 1060; gekoppeld in `vortex-context`).
#[async_trait::async_trait]
pub trait Reranker: Send + Sync + std::fmt::Debug {
    async fn rerank(&self, query: &str, docs: &[String]) -> Result<Vec<f32>>;
}

#[derive(Debug, Clone, Serialize)]
pub struct Query {
    pub text: String,
    pub filters: Filters,
    pub k: usize,
    /// Reserveer een plek voor tegenbewijs.
    pub counter_evidence: bool,
    /// Gewichten van de eindrangschikking.
    pub w_relevance: f64,
    pub w_recency: f64,
    pub w_importance: f64,
    /// MMR: 1 = puur relevantie, 0 = puur diversiteit.
    pub mmr_lambda: f32,
    /// Gepersonaliseerd ophalen: deze typen krijgen een kleine bonus (bv. user/project bij
    /// persoonlijke vragen).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefer_kinds: Vec<MemoryKind>,
    /// Run waarvoor opgehaald wordt (komt in het retrieval-event → GUI-paneel).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<vortex_types::RunId>,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            filters: Filters::default(),
            k: 8,
            counter_evidence: true,
            w_relevance: 0.65,
            w_recency: 0.15,
            w_importance: 0.20,
            mmr_lambda: 0.75,
            prefer_kinds: vec![],
            run_id: None,
        }
    }
}

/// FTS5-veilige zoekterm: woorden als OR-termen tussen aanhalingstekens.
pub fn fts_query(text: &str) -> String {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1)
        .take(24)
        .map(|w| format!("\"{}\"", w.replace('"', "")))
        .collect();
    if words.is_empty() { "\"\"".into() } else { words.join(" OR ") }
}

/// Tijdsaanduidingen in de vraag → venster (since, until).
pub fn temporal_window(text: &str, now: chrono::DateTime<Utc>) -> Option<(String, String)> {
    let l = text.to_lowercase();
    let day = |d: chrono::DateTime<Utc>| d.date_naive().and_hms_opt(0, 0, 0).map(|x| x.and_utc());
    let (from, to) = if l.contains("vandaag") || l.contains("today") {
        (day(now)?, now)
    } else if l.contains("gisteren") || l.contains("yesterday") {
        let y = now - CDuration::days(1);
        (day(y)?, day(now)?)
    } else if l.contains("vorige week") || l.contains("last week") {
        (now - CDuration::days(14), now - CDuration::days(7))
    } else if l.contains("deze week") || l.contains("this week") {
        (now - CDuration::days(now.weekday().num_days_from_monday() as i64 + 1), now)
    } else if l.contains("vorige maand") || l.contains("last month") {
        (now - CDuration::days(60), now - CDuration::days(28))
    } else if l.contains("deze maand") || l.contains("this month") {
        (now - CDuration::days(now.day() as i64), now)
    } else if l.contains("recent") || l.contains("laatst") || l.contains("onlangs") {
        (now - CDuration::days(7), now)
    } else {
        return None;
    };
    Some((from.to_rfc3339(), to.to_rfc3339()))
}

fn recency(created: &str, halflife_days: f64) -> f64 {
    chrono::DateTime::parse_from_rfc3339(created)
        .map(|t| {
            let age = (Utc::now() - t.with_timezone(&Utc)).num_seconds().max(0) as f64 / 86400.0;
            0.5f64.powf(age / halflife_days.max(0.01))
        })
        .unwrap_or(0.5)
}

impl MemoryStore {
    /// Hybride ophalen. Optioneel met reranker.
    pub async fn retrieve(&self, q: &Query, reranker: Option<&Arc<dyn Reranker>>) -> Result<Vec<Retrieved>> {
        let trace = TraceId::new();
        let mut filters = q.filters.clone();
        let mut temporal = false;
        if filters.since.is_none()
            && filters.until.is_none()
            && let Some((s, u)) = temporal_window(&q.text, Utc::now())
        {
            filters.since = Some(s);
            filters.until = Some(u);
            temporal = true;
        }
        let rewrites = self.llm.rewrite_query(&q.text).await.unwrap_or_else(|_| vec![q.text.clone()]);
        let pool = (q.k * 5).max(30);
        let meta = self.meta_snapshot();
        let allow = |id: &MemoryId| {
            meta.get(id).is_some_and(|m| {
                (m.workspace.is_none() || m.workspace == filters.workspace)
                    && (filters.kinds.is_empty() || filters.kinds.contains(&m.kind))
                    && (filters.include_stale || !m.stale)
                    && filters.min_confidence.is_none_or(|c| m.confidence >= c)
                    && filters.since.as_ref().is_none_or(|s| m.created_at.as_str() >= s.as_str())
                    && filters.until.as_ref().is_none_or(|u| m.created_at.as_str() <= u.as_str())
                    && filters
                        .source_prefix
                        .as_ref()
                        .is_none_or(|p| m.source_uri.as_deref().is_some_and(|s| s.starts_with(p.as_str())))
            })
        };

        // 1. Vector (alle varianten).
        let mut parts: HashMap<MemoryId, ScoreParts> = HashMap::new();
        let mut qvecs: Vec<Vec<f32>> = Vec::new();
        // Embeddingmodel weg → zonder vectorstap verder (BM25 + graph), niet stil: tracing + flag.
        let (embs, vector_ok) = match self.embedder.embed(&rewrites).await {
            Ok(e) => (e, true),
            Err(e) => {
                tracing::warn!(error = %e, "embeddings niet beschikbaar; ophalen zonder vectorstap");
                (vec![], false)
            }
        };
        for e in embs {
            let v = normalize(e, self.cfg.dims);
            for (rank, (id, s)) in self.index().search(&v, pool, &allow).into_iter().enumerate() {
                let p = parts.entry(id).or_default();
                p.vector = Some(p.vector.map_or(s, |x| x.max(s)));
                p.rrf += 1.0 / (60.0 + rank as f64);
            }
            qvecs.push(v);
        }
        // 2. BM25.
        let mut sql = format!("{} JOIN memories_fts f ON f.rowid = m.rowid WHERE memories_fts MATCH ?", mem_select());
        let mut args: Vec<rusqlite::types::Value> = vec![fts_query(&rewrites.join(" ")).into()];
        Self::filter_sql(&filters, &mut sql, &mut args);
        sql.push_str(" ORDER BY bm25(memories_fts) LIMIT ?");
        args.push((pool as i64).into());
        let bm: Vec<Memory> = self.log.db().read(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), row_mem)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        for (rank, m) in bm.iter().enumerate() {
            let p = parts.entry(m.id).or_default();
            p.bm25_rank = Some(rank + 1);
            p.rrf += 1.0 / (60.0 + rank as f64);
        }
        // 3. Graph: entiteiten uit de vraag → herinneringen (hop 1) → buren (hop 2).
        let lower = q.text.to_lowercase();
        let graph: Vec<(MemoryId, u32)> = self.log.db().read(|c| {
            let mut st = c.prepare("SELECT id, norm FROM entities")?;
            let ents: Vec<(String, String)> =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            let hit: Vec<String> = ents
                .into_iter()
                .filter(|(_, n)| {
                    n.len() > 2 && lower.split(|ch: char| !ch.is_alphanumeric()).any(|w| w == n)
                        || (n.contains(' ') && lower.contains(n.as_str()))
                })
                .map(|(id, _)| id)
                .collect();
            let mut out = Vec::new();
            let mut st_m = c.prepare("SELECT memory_id FROM memory_entities WHERE entity_id = ?1 LIMIT 50")?;
            let mut st_n =
                c.prepare("SELECT dst FROM edges WHERE src = ?1 UNION SELECT src FROM edges WHERE dst = ?1 LIMIT 20")?;
            let mut seen_e: HashSet<String> = HashSet::new();
            for e in &hit {
                seen_e.insert(e.clone());
                for m in st_m.query_map([e], |r| r.get::<_, String>(0))? {
                    out.push((m?, 1));
                }
            }
            for e in &hit {
                let nbrs: Vec<String> = st_n.query_map([e], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                for n in nbrs {
                    if seen_e.insert(n.clone()) {
                        for m in st_m.query_map([&n], |r| r.get::<_, String>(0))? {
                            out.push((m?, 2));
                        }
                    }
                }
            }
            Ok(out.into_iter().filter_map(|(m, h)| m.parse().ok().map(|id| (id, h))).collect())
        })?;
        for (rank, (id, hops)) in graph.into_iter().filter(|(id, _)| allow(id)).enumerate() {
            let p = parts.entry(id).or_default();
            p.graph_hops = Some(p.graph_hops.map_or(hops, |x| x.min(hops)));
            p.rrf += 0.8 / (60.0 + rank as f64 + 10.0 * (hops as f64 - 1.0));
        }
        if parts.is_empty() {
            return Ok(vec![]);
        }
        // Kandidaten laden.
        let mut cands: Vec<(Memory, ScoreParts)> = Vec::new();
        let mut ranked: Vec<(MemoryId, ScoreParts)> = parts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.rrf.total_cmp(&a.1.rrf));
        ranked.truncate(pool);
        for (id, mut p) in ranked {
            if let Ok(m) = self.get(id) {
                p.temporal = temporal;
                cands.push((m, p));
            }
        }
        // 4. Rerank.
        if let Some(r) = reranker {
            let docs: Vec<String> = cands.iter().map(|(m, _)| format!("{}\n{}", m.context, m.content)).collect();
            if let Ok(scores) = r.rerank(&q.text, &docs).await
                && scores.len() == cands.len()
            {
                for ((_, p), s) in cands.iter_mut().zip(scores) {
                    p.rerank = Some(s);
                }
            }
        }
        // 5. Eindscore.
        let scorer = self.scorer();
        let max_rrf = cands.iter().map(|(_, p)| p.rrf).fold(0.0, f64::max).max(1e-9);
        for (m, p) in cands.iter_mut() {
            let rel = match p.rerank {
                Some(r) => r as f64,
                None => 0.5 * (p.rrf / max_rrf) + 0.5 * p.vector.map_or(0.0, |v| v.max(0.0) as f64),
            };
            p.recency = recency(m.event_time.as_deref().unwrap_or(&m.created_at), self.cfg.halflife_days);
            p.importance = m.importance;
            p.final_score =
                q.w_relevance * rel + q.w_recency * p.recency + q.w_importance * p.importance * m.confidence.max(0.1);
            // Usefulness-predictor (W11): kans dat deze herinnering helpt (actief net) of de heuristiek.
            let reference = format!("{}:{}", q.run_id.map(|r| r.to_string()).unwrap_or_default(), m.id);
            let out = vortex_core::neural::decide(
                &scorer,
                "usefulness",
                &crate::features::usefulness_vector(p),
                vec![p.final_score as f32],
                &reference,
            );
            if let Some(v) = out.first() {
                p.final_score = f64::from(*v);
            }
        }
        cands.sort_by(|a, b| b.1.final_score.total_cmp(&a.1.final_score));
        // 6. MMR (diversiteit) op vectoren.
        let vecs: HashMap<MemoryId, Vec<f32>> = {
            let ids: Vec<MemoryId> = cands.iter().map(|(m, _)| m.id).collect();
            self.vectors_of(&ids)?
        };
        let k = q.k.max(1);
        let slots = if q.counter_evidence { k.saturating_sub(1).max(1) } else { k };
        let mut chosen: Vec<usize> = Vec::new();
        let mut rest: Vec<usize> = (0..cands.len()).collect();
        while chosen.len() < slots && !rest.is_empty() {
            let mut best = (f64::MIN, 0usize);
            for (pos, &i) in rest.iter().enumerate() {
                let sim_max = chosen
                    .iter()
                    .filter_map(|&j| Some(cos(vecs.get(&cands[i].0.id)?, vecs.get(&cands[j].0.id)?)))
                    .fold(0f32, f32::max) as f64;
                let s = q.mmr_lambda as f64 * cands[i].1.final_score - (1.0 - q.mmr_lambda as f64) * sim_max;
                if s > best.0 {
                    best = (s, pos);
                }
            }
            chosen.push(rest.remove(best.1));
        }
        let mut out: Vec<Retrieved> = chosen
            .iter()
            .map(|&i| Retrieved { memory: cands[i].0.clone(), score: cands[i].1.clone(), counter_evidence: false })
            .collect();
        // 7. Tegenbewijs: conflict met (of tegengestelde claim t.o.v.) een gekozen item.
        if q.counter_evidence && !out.is_empty() {
            let chosen_ids: HashSet<MemoryId> = out.iter().map(|r| r.memory.id).collect();
            let against = self.counter_candidates(&chosen_ids)?;
            if let Some((id, _)) = against.into_iter().find(|(id, _)| !chosen_ids.contains(id) && allow(id))
                && let Ok(m) = self.get(id)
            {
                out.push(Retrieved { memory: m, score: ScoreParts::default(), counter_evidence: true });
            }
        }
        let ids: Vec<MemoryId> = out.iter().map(|r| r.memory.id).collect();
        self.touch(&ids)?;
        // Event met features: trainingsdata voor reranker/usefulness (W11).
        let mut ev = Event::new(
            EventKind::RetrievalPerformed,
            trace,
            serde_json::json!({
                "vector_stage": vector_ok,
                "query_chars": q.text.len(),
                "rewrites": rewrites.len(),
                "temporal": temporal,
                "candidates": cands.len(),
                "items": out.iter().map(|r| serde_json::json!({"id": r.memory.id, "kind": r.memory.kind, "score": r.score, "counter": r.counter_evidence})).collect::<Vec<_>>(),
            }),
        );
        ev.run_id = q.run_id;
        let _ = self.log.append(ev);
        Ok(out)
    }

    fn vectors_of(&self, ids: &[MemoryId]) -> Result<HashMap<MemoryId, Vec<f32>>> {
        let g = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        Ok(self.log.db().read(|c| {
            let mut st = c.prepare("SELECT scale, data FROM vectors WHERE memory_id = ?1 AND generation = ?2")?;
            let mut out = HashMap::new();
            for id in ids {
                if let Ok((s, d)) = st.query_row(rusqlite::params![id.to_string(), g], |r| {
                    Ok((r.get::<_, f64>(0)?, r.get::<_, Vec<u8>>(1)?))
                }) {
                    out.insert(*id, d.into_iter().map(|b| (b as i8) as f32 * s as f32).collect());
                }
            }
            Ok(out)
        })?)
    }

    /// Herinneringen die een van `ids` tegenspreken: open conflicten of tegengestelde claims.
    fn counter_candidates(&self, ids: &HashSet<MemoryId>) -> Result<Vec<(MemoryId, String)>> {
        Ok(self.log.db().read(|c| {
            let mut out = Vec::new();
            let mut st = c.prepare("SELECT a_memory, b_memory, reason FROM conflicts WHERE status != 'resolved' AND (a_memory = ?1 OR b_memory = ?1)")?;
            let mut st_c = c.prepare(
                "SELECT c2.memory_id FROM claims c1 JOIN claims c2 ON c1.subject = c2.subject AND c1.predicate = c2.predicate
                 WHERE c1.memory_id = ?1 AND c2.memory_id != ?1 AND (c1.polarity != c2.polarity OR c1.object != c2.object)",
            )?;
            for id in ids {
                let s = id.to_string();
                for r in st.query_map([&s], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
                    let (a, b, reason) = r?;
                    let other = if a == s { b } else { a };
                    if let Ok(o) = other.parse() {
                        out.push((o, reason));
                    }
                }
                for r in st_c.query_map([&s], |r| r.get::<_, String>(0))? {
                    if let Ok(o) = r?.parse() {
                        out.push((o, "tegengestelde claim".to_string()));
                    }
                }
            }
            Ok(out)
        })?)
    }
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_is_safe() {
        assert_eq!(fts_query("wat is \"x\" AND y?"), "\"wat\" OR \"is\" OR \"AND\"");
        assert_eq!(fts_query("?!"), "\"\"");
    }

    #[test]
    fn temporal_words_give_windows() {
        let now = Utc::now();
        assert!(temporal_window("wat deed ik gisteren?", now).is_some());
        assert!(temporal_window("wat is een kat?", now).is_none());
        let (s, u) = temporal_window("vandaag", now).unwrap_or_default();
        assert!(s <= u);
    }
}
