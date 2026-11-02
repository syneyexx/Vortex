//! Tiering SSD/HDD (W12.07): weinig gebruikte herinneringen (decay-gestuurd, W11-layer of
//! heuristiek) gaan in zstd-blokken naar de koude schijf; op de SSD blijft een stub (titel +
//! begin). Zoeken: centroid per blok → beste blokken laden (via een SSD-cache, LRU) → cosinus →
//! treffers worden teruggehaald (warm). Buurblokken worden vooruit in de cache gezet (prefetch).
// VX:W12.07

use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventId, EventKind, MemoryId, TraceId};

use crate::error::{MemError, Result};
use crate::store::MemoryStore;
use crate::types::MemoryKind;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierConfig {
    /// Koude schijf (HDD).
    pub cold_dir: PathBuf,
    /// SSD-cache voor recent gelezen blokken.
    pub cache_dir: PathBuf,
    pub block_size: usize,
    pub cache_blocks: usize,
    /// Pas koud na zoveel dagen zonder gebruik (heuristiek).
    pub idle_days: i64,
}

impl TierConfig {
    pub fn new(cold_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        Self { cold_dir: cold_dir.into(), cache_dir: cache_dir.into(), block_size: 64, cache_blocks: 64, idle_days: 60 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ColdItem {
    id: String,
    content: String,
    scale: f32,
    data: Vec<i8>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TierStats {
    pub hot: i64,
    pub cold: i64,
    pub blocks: i64,
    pub cold_bytes: i64,
    pub cached_blocks: usize,
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

impl MemoryStore {
    pub fn set_tiering(&self, cfg: Option<TierConfig>) {
        if let Some(c) = &cfg {
            let _ = std::fs::create_dir_all(c.cold_dir.join("blocks"));
            let _ = std::fs::create_dir_all(&c.cache_dir);
        }
        *self.tier.write() = cfg;
    }

    fn tier_cfg(&self) -> Result<TierConfig> {
        self.tier.read().clone().ok_or_else(|| MemError::Invalid("tiering niet ingesteld".into()))
    }

    /// Kandidaten voor de koude schijf: lang niet gebruikt, geen gebruikers-/project-/werkwijze-
    /// kennis, en de decay-predictor (of heuristiek) verwacht geen gebruik meer.
    pub fn cold_candidates(&self, max: usize) -> Result<Vec<MemoryId>> {
        let cfg = self.tier_cfg()?;
        let cutoff = (vortex_types::now() - chrono::Duration::days(cfg.idle_days)).to_rfc3339();
        let rows = self.list(&crate::types::Filters::default(), None, 50_000, 0)?;
        let now = chrono::Utc::now();
        let scorer = self.scorer();
        Ok(rows
            .into_iter()
            .filter(|m| {
                !matches!(
                    m.kind,
                    MemoryKind::User
                        | MemoryKind::Project
                        | MemoryKind::Procedural
                        | MemoryKind::Summary
                        | MemoryKind::Meta
                )
            })
            .filter(|m| m.updated_at < cutoff)
            .filter(|m| {
                let h = if m.access_count > 2 || m.importance >= 0.7 { 1.0 } else { 0.0 };
                vortex_core::neural::decide(
                    &scorer,
                    "decay",
                    &crate::features::decay_vector(m, now),
                    vec![h],
                    &m.id.to_string(),
                )
                .first()
                .is_some_and(|v| *v < 0.5)
            })
            .take(max)
            .map(|m| m.id)
            .collect())
    }

    /// Naar de koude schijf (zstd-blokken). Geeft het aantal verplaatste herinneringen.
    pub fn demote(&self, ids: &[MemoryId]) -> Result<usize> {
        let cfg = self.tier_cfg()?;
        let vecs = self.vectors_of(ids)?;
        let g = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        let mut moved = 0;
        for chunk in ids.chunks(cfg.block_size.max(1)) {
            let mut items = Vec::new();
            let mut centroid = vec![0f32; self.cfg.dims];
            for id in chunk {
                let Ok(m) = self.get(*id) else { continue };
                let Some(v) = vecs.get(id) else { continue };
                if m.stale {
                    continue;
                }
                for (c, x) in centroid.iter_mut().zip(v) {
                    *c += x;
                }
                let (q, scale) = crate::vector::quantize(v);
                items.push(ColdItem { id: id.to_string(), content: m.content.clone(), scale, data: q });
            }
            if items.is_empty() {
                continue;
            }
            let n = (centroid.iter().map(|x| x * x).sum::<f32>()).sqrt().max(1e-9);
            centroid.iter_mut().for_each(|x| *x /= n);
            let bid = format!("blk_{}", EventId::new().to_string().trim_start_matches("evt_"));
            let path = cfg.cold_dir.join("blocks").join(format!("{bid}.zst"));
            let raw = serde_json::to_vec(&items)?;
            let comp = zstd::encode_all(raw.as_slice(), 9).map_err(|e| MemError::Invalid(format!("zstd: {e}")))?;
            std::fs::write(&path, &comp).map_err(|e| MemError::Invalid(format!("{}: {e}", path.display())))?;
            let cbytes: Vec<u8> = centroid.iter().flat_map(|x| x.to_le_bytes()).collect();
            let now = vortex_types::now().to_rfc3339();
            self.log.transact(|tx, out| {
                tx.execute(
                    "INSERT INTO cold_blocks(id, path, n, bytes, centroid, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![bid, path.display().to_string(), items.len() as i64, comp.len() as i64, cbytes, now],
                )?;
                for (slot, it) in items.iter().enumerate() {
                    tx.execute(
                        "INSERT OR REPLACE INTO cold_members(memory_id, block_id, slot) VALUES (?1, ?2, ?3)",
                        params![it.id, bid, slot as i64],
                    )?;
                    // Stub op de SSD: titel + begin; vector weg uit de warme index.
                    let stub: String = it.content.chars().take(160).collect();
                    tx.execute(
                        "UPDATE memories SET tier = 'cold', content = ?2 WHERE id = ?1",
                        params![it.id, format!("{stub}…")],
                    )?;
                    tx.execute("DELETE FROM vectors WHERE memory_id = ?1 AND generation = ?2", params![it.id, g])?;
                }
                out.emit(
                    tx,
                    Event::new(
                        EventKind::MemoryTiered,
                        TraceId::new(),
                        serde_json::json!({"to": "cold", "block": bid, "n": items.len(), "bytes": comp.len()}),
                    ),
                )?;
                Ok(())
            })?;
            for it in &items {
                if let Ok(id) = it.id.parse() {
                    self.index().remove(&id);
                }
            }
            moved += items.len();
        }
        self.bump();
        Ok(moved)
    }

    /// Blok lezen: eerst uit de SSD-cache, anders van de koude schijf (en in de cache zetten).
    fn load_block(&self, block_id: &str, path: &Path) -> Result<Vec<ColdItem>> {
        let cfg = self.tier_cfg()?;
        let cached = cfg.cache_dir.join(format!("{block_id}.zst"));
        let bytes = match std::fs::read(&cached) {
            Ok(b) => b,
            Err(_) => {
                let b = std::fs::read(path).map_err(|e| MemError::Invalid(format!("{}: {e}", path.display())))?;
                let _ = std::fs::write(&cached, &b);
                let mut lru = self.tier_cache.lock();
                lru.push_back(block_id.to_string());
                while lru.len() > cfg.cache_blocks {
                    if let Some(old) = lru.pop_front() {
                        let _ = std::fs::remove_file(cfg.cache_dir.join(format!("{old}.zst")));
                    }
                }
                b
            }
        };
        let raw = zstd::decode_all(bytes.as_slice()).map_err(|e| MemError::Invalid(format!("zstd: {e}")))?;
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "UPDATE cold_blocks SET last_read = ?2 WHERE id = ?1",
                params![block_id, vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        });
        Ok(serde_json::from_slice(&raw)?)
    }

    /// Zoeken in de koude laag: beste `blocks` blokken op centroid, dan cosinus per item.
    pub fn cold_search(&self, qvec: &[f32], k: usize, blocks: usize) -> Result<Vec<(MemoryId, f32)>> {
        let rows: Vec<(String, String, Vec<u8>)> = self.log.db().read(|c| {
            let mut st = c.prepare("SELECT id, path, centroid FROM cold_blocks")?;
            let v =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?;
        if rows.is_empty() {
            return Ok(vec![]);
        }
        let mut scored: Vec<(f32, &String, &String)> =
            rows.iter().map(|(id, p, c)| (cosine(qvec, &f32s(c)), id, p)).collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        // Prefetch: de volgende blokken alvast in de SSD-cache (achtergrondthread).
        if let Ok(cfg) = self.tier_cfg() {
            let next: Vec<(String, String)> =
                scored.iter().skip(blocks).take(2).map(|(_, i, p)| ((*i).clone(), (*p).clone())).collect();
            std::thread::spawn(move || {
                for (id, p) in next {
                    let c = cfg.cache_dir.join(format!("{id}.zst"));
                    if !c.exists()
                        && let Ok(b) = std::fs::read(&p)
                    {
                        let _ = std::fs::write(c, b);
                    }
                }
            });
        }
        let members: std::collections::HashSet<String> = self.log.db().read(|c| {
            let mut st = c.prepare("SELECT memory_id FROM cold_members")?;
            let v = st
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
            Ok(v)
        })?;
        let mut hits = Vec::new();
        for (_, id, p) in scored.into_iter().take(blocks) {
            for it in self.load_block(id, Path::new(p))? {
                if !members.contains(&it.id) {
                    continue; // al teruggehaald
                }
                let v: Vec<f32> = it.data.iter().map(|b| f32::from(*b) * it.scale).collect();
                if let Ok(mid) = it.id.parse() {
                    hits.push((mid, cosine(qvec, &v)));
                }
            }
        }
        hits.sort_by(|a, b| b.1.total_cmp(&a.1));
        hits.truncate(k);
        Ok(hits)
    }

    /// Terughalen naar warm: volledige inhoud + vector + index.
    pub fn rehydrate(&self, id: MemoryId) -> Result<bool> {
        let row: Option<(String, String)> = self.log.db().read(|c| {
            Ok(rusqlite::OptionalExtension::optional(c.query_row(
                "SELECT b.id, b.path FROM cold_members m JOIN cold_blocks b ON b.id = m.block_id WHERE m.memory_id = ?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ))?)
        })?;
        let Some((bid, path)) = row else { return Ok(false) };
        let items = self.load_block(&bid, Path::new(&path))?;
        let Some(it) = items.into_iter().find(|i| i.id == id.to_string()) else { return Ok(false) };
        let g = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        self.log.transact(|tx, out| {
            tx.execute(
                "UPDATE memories SET tier = 'hot', content = ?2, updated_at = ?3 WHERE id = ?1",
                params![it.id, it.content, vortex_types::now().to_rfc3339()],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO vectors(memory_id, generation, scale, data) VALUES (?1, ?2, ?3, ?4)",
                params![it.id, g, f64::from(it.scale), it.data.iter().map(|b| *b as u8).collect::<Vec<u8>>()],
            )?;
            tx.execute("DELETE FROM cold_members WHERE memory_id = ?1", [&it.id])?;
            out.emit(
                tx,
                Event::new(EventKind::MemoryTiered, TraceId::new(), serde_json::json!({"to": "hot", "memory": it.id})),
            )?;
            Ok(())
        })?;
        self.index().upsert(id, it.data, it.scale);
        self.bump();
        Ok(true)
    }

    pub fn tier_stats(&self) -> Result<TierStats> {
        let cached = self.tier_cache.lock().len();
        Ok(self.log.db().read(|c| {
            Ok(TierStats {
                hot: c.query_row("SELECT COUNT(*) FROM memories WHERE tier = 'hot' AND stale = 0", [], |r| r.get(0))?,
                cold: c.query_row("SELECT COUNT(*) FROM cold_members", [], |r| r.get(0))?,
                blocks: c.query_row("SELECT COUNT(*) FROM cold_blocks", [], |r| r.get(0))?,
                cold_bytes: c.query_row("SELECT COALESCE(SUM(bytes), 0) FROM cold_blocks", [], |r| r.get(0))?,
                cached_blocks: cached,
            })
        })?)
    }

    /// Meet zoeklatency over de koude laag (voor de 🔒 TB-simulatie: `vortex tierbench`).
    pub fn cold_latency(&self, qvec: &[f32]) -> Result<(u128, usize)> {
        let t = Instant::now();
        let n = self.cold_search(qvec, 10, 3)?.len();
        Ok((t.elapsed().as_micros(), n))
    }
}
