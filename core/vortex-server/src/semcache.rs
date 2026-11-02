//! Semantische antwoordcache (W17.11). Strikt: alleen informatieve vragen (geen persoonlijke,
//! tijdgevoelige of research-vragen), geen tools of bijlagen, nooit voor acties; geldig zolang
//! model, configversie en kennis (geheugengeneratie zonder chatgeschiedenis) gelijk zijn;
//! treffer = zelfde genormaliseerde vraag, of embedding-cosinus ≥ 0,97 én woordoverlap ≥ 0,8.
//! Stampede: gelijke vragen tegelijk → één generatie, de rest wacht en leest de cache.
// VX:W17.11

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::params;
use serde_json::json;
use tokio::sync::Notify;
use vortex_core::EventLog;

pub const MIN_COS: f32 = 0.97;
pub const MIN_JACCARD: f64 = 0.8;
const TTL_DAYS: i64 = 7;

pub fn norm(q: &str) -> String {
    q.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

fn words(q: &str) -> BTreeSet<String> {
    norm(q).split(' ').filter(|w| w.len() > 1).map(String::from).collect()
}

fn jaccard(a: &str, b: &str) -> f64 {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / a.union(&b).count() as f64
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

/// Kennisgeneratie: verandert als blijvende kennis verandert (niet door gewone chatbeurten).
pub fn mem_gen(log: &EventLog) -> String {
    log.db()
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), '') FROM memories WHERE stale = 0 AND kind IN ('user','project','semantic','world','procedural') AND COALESCE(source_uri, '') NOT LIKE 'chat:%'",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub id: String,
    pub answer: String,
    pub similarity: f32,
}

#[derive(Debug, Default)]
pub struct SemCache {
    inflight: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

/// Claim op een sleutel: eigenaar genereert (los bij drop), anderen wachten.
#[derive(Debug)]
pub struct Claim {
    inflight: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    key: String,
    notify: Arc<Notify>,
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.inflight.lock().remove(&self.key);
        self.notify.notify_waiters();
    }
}

impl SemCache {
    /// Eigenaar worden, of wachten (max `wait`) tot de eigenaar klaar is.
    pub async fn claim(&self, key: &str, wait: Duration) -> Option<Claim> {
        let existing = {
            let mut g = self.inflight.lock();
            match g.get(key) {
                Some(n) => Some(n.clone()),
                None => {
                    let n = Arc::new(Notify::new());
                    g.insert(key.to_string(), n.clone());
                    return Some(Claim { inflight: self.inflight.clone(), key: key.to_string(), notify: n });
                }
            }
        };
        if let Some(n) = existing {
            let _ = tokio::time::timeout(wait, n.notified()).await;
        }
        None
    }

    pub fn lookup(
        &self,
        log: &EventLog,
        q: &str,
        emb: Option<&[f32]>,
        model: &str,
        cfg: &str,
        generation: &str,
    ) -> Option<Hit> {
        let since = (vortex_types::now() - chrono::Duration::days(TTL_DAYS)).to_rfc3339();
        let rows: Vec<(String, String, String, String)> = log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id, question, embedding, answer FROM answer_cache WHERE model = ?1 AND cfg_version = ?2 AND mem_gen = ?3 AND created_at >= ?4 ORDER BY created_at DESC LIMIT 500")?;
                let v = st.query_map(params![model, cfg, generation, since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let n = norm(q);
        let best = rows
            .iter()
            .filter_map(|(id, question, e, a)| {
                if norm(question) == n {
                    return Some((id, a, 1.0));
                }
                let e: Vec<f32> = serde_json::from_str(e).ok()?;
                let s = cos(emb?, &e);
                (s >= MIN_COS && jaccard(q, question) >= MIN_JACCARD).then_some((id, a, s))
            })
            .max_by(|a, b| a.2.total_cmp(&b.2))?;
        let _ = log.db().write(|tx| Ok(tx.execute("UPDATE answer_cache SET hits = hits + 1 WHERE id = ?1", [best.0])?));
        Some(Hit { id: best.0.clone(), answer: best.1.clone(), similarity: best.2 })
    }

    #[allow(clippy::too_many_arguments)] // alle sleuteldelen van een cache-regel
    pub fn store(
        &self,
        log: &EventLog,
        q: &str,
        emb: Option<&[f32]>,
        answer: &str,
        model: &str,
        cfg: &str,
        generation: &str,
        run: &str,
        msg: &str,
    ) {
        let id = format!(
            "ac_{:016x}",
            vortex_tools::stable_hash(format!("{}|{model}|{cfg}|{generation}", norm(q)).as_bytes())
        );
        let _ = log.db().write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO answer_cache(id, norm, question, embedding, answer, model, cfg_version, mem_gen, run_id, message_id, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![id, norm(q), q, json!(emb.unwrap_or(&[])).to_string(), answer, model, cfg, generation, run, msg, vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        });
    }

    /// Negatieve feedback op een (gecachet of bron-)antwoord → weg uit de cache.
    pub fn invalidate_message(&self, log: &EventLog, msg: &str) -> usize {
        log.db().write(|tx| Ok(tx.execute("DELETE FROM answer_cache WHERE message_id = ?1 OR id IN (SELECT json_extract(payload, '$.cache_id') FROM events WHERE kind = 'chat.cache.hit' AND json_extract(payload, '$.message_id') = ?1)", [msg])?)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_matching() {
        assert_eq!(norm("Wat is de hoofdstad van Frankrijk?"), norm("wat is de  hoofdstad van frankrijk"));
        assert!(jaccard("Wat is de hoofdstad van Frankrijk?", "wat is de hoofdstad van Duitsland") < MIN_JACCARD);
        assert!(cos(&[1.0, 0.0], &[1.0, 0.0]) > 0.99);
    }

    #[tokio::test]
    async fn stampede_one_owner() {
        let c = Arc::new(SemCache::default());
        let owner = c.claim("k", Duration::from_secs(1)).await;
        assert!(owner.is_some());
        let c2 = c.clone();
        let w = tokio::spawn(async move { c2.claim("k", Duration::from_secs(5)).await.is_none() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(owner);
        assert!(w.await.unwrap_or(false), "wachter kreeg geen claim maar werd gewekt");
        assert!(c.claim("k", Duration::from_secs(1)).await.is_some(), "daarna vrij");
    }
}
