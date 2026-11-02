//! De `MemoryStore`: schrijfpipeline, lezen, bewerken, vergeten (met doorwerking), bronnen,
//! conflicten, werkruimtes, export/import en backup. SQLite is de waarheid; de vectorindex en
//! metadata-cache in RAM zijn afgeleid en worden bij openen opnieuw opgebouwd.
// VX:W05.01

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use parking_lot::RwLock;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use vortex_core::config::fnv1a64;
use vortex_core::{Db, EventLog};
use vortex_types::{DecisionRecord, Event, EventId, EventKind, MemoryId, Provenance, TraceId};

use crate::chunk::chunk;
use crate::embed::Embedder;
use crate::error::{MemError, Result};
use crate::llm::{Extraction, MemLlm};
use crate::policy;
use crate::types::*;
use crate::vector::{FlatI8Index, VectorIndex, quantize};

/// Instellingen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemConfig {
    pub dims: usize,
    /// Cosinus vanaf waar twee stukken als hetzelfde gelden (samenvoegen).
    pub dedup_threshold: f32,
    /// Cosinus vanaf waar we op tegenstrijdigheid controleren.
    pub conflict_threshold: f32,
    pub min_importance: f64,
    pub chunk_chars: usize,
    /// Langer dan dit → automatische samenvatting.
    pub summarize_over_chars: usize,
    pub halflife_days: f64,
}

impl Default for MemConfig {
    fn default() -> Self {
        Self {
            dims: 512,
            dedup_threshold: 0.95,
            conflict_threshold: 0.75,
            min_importance: 0.35,
            chunk_chars: 1200,
            summarize_over_chars: 900,
            halflife_days: 30.0,
        }
    }
}

/// Kleine metadata per herinnering in RAM (voor snelle filters bij vectorzoeken).
#[derive(Debug, Clone)]
pub(crate) struct Meta {
    pub kind: MemoryKind,
    pub workspace: Option<String>,
    pub created_at: String,
    pub confidence: f64,
    pub stale: bool,
    pub source_uri: Option<String>,
}

#[derive(Debug)]
pub struct MemoryStore {
    pub log: EventLog,
    pub(crate) embedder: Arc<dyn Embedder>,
    pub(crate) llm: Arc<dyn MemLlm>,
    pub(crate) index: RwLock<Arc<dyn VectorIndex>>,
    pub(crate) meta: RwLock<HashMap<MemoryId, Meta>>,
    pub(crate) generation: AtomicI64,
    pub(crate) cfg: MemConfig,
    /// Verhoogd bij elke wijziging: caches (bv. retrieval-resultaten) worden ongeldig.
    pub(crate) cache_gen: AtomicU64,
}

pub(crate) fn mem_select() -> &'static str {
    "SELECT m.id, m.kind, m.workspace_id, m.session_id, m.title, m.content, m.context, m.summary, s.uri, m.locator,
        m.provenance, m.importance, m.salience, m.confidence, m.valid_from, m.valid_to, m.event_time, m.access_count,
        m.version, m.stale, m.merged_count, m.tags, m.created_at, m.updated_at
     FROM memories m LEFT JOIN sources s ON s.id = m.source_id"
}

pub(crate) fn row_mem(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    fn conv<T, E: std::fmt::Display>(i: usize, v: std::result::Result<T, E>) -> rusqlite::Result<T> {
        v.map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.to_string().into()))
    }
    let kind: String = r.get(1)?;
    let prov: String = r.get(10)?;
    let tags: String = r.get(21)?;
    Ok(Memory {
        id: conv(0, r.get::<_, String>(0)?.parse())?,
        kind: conv(1, MemoryKind::parse(&kind).ok_or("type"))?,
        workspace: r.get(2)?,
        session_id: r.get(3)?,
        title: r.get(4)?,
        content: r.get(5)?,
        context: r.get(6)?,
        summary: r.get(7)?,
        source_uri: r.get(8)?,
        locator: r.get(9)?,
        provenance: conv(10, serde_json::from_str::<Provenance>(&prov))?,
        importance: r.get(11)?,
        salience: r.get(12)?,
        confidence: r.get(13)?,
        valid_from: r.get(14)?,
        valid_to: r.get(15)?,
        event_time: r.get(16)?,
        access_count: r.get(17)?,
        version: r.get(18)?,
        stale: r.get::<_, i64>(19)? != 0,
        merged_count: r.get(20)?,
        tags: conv(21, serde_json::from_str(&tags))?,
        created_at: r.get(22)?,
        updated_at: r.get(23)?,
    })
}

fn now() -> String {
    vortex_types::now().to_rfc3339()
}

fn norm_entity(s: &str) -> String {
    s.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Rij voor export/import (JSONL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRow {
    pub memory: Memory,
    pub claims: Vec<crate::llm::ExtractedClaim>,
    pub entities: Vec<String>,
}

impl MemoryStore {
    /// Open op een bestaande database. Laadt de actieve indexgeneratie (of maakt er één).
    pub fn open(log: EventLog, embedder: Arc<dyn Embedder>, llm: Arc<dyn MemLlm>, cfg: MemConfig) -> Result<Self> {
        let dims = cfg.dims.min(embedder.dims());
        let cfg = MemConfig { dims, ..cfg };
        let model = embedder.model().to_string();
        let generation = log.db().write(|tx| {
            let active: Option<(i64, String, i64)> = tx
                .query_row("SELECT generation, model, dims FROM index_generations WHERE active = 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?;
            match active {
                Some((g, m, d)) if m == model && d as usize == dims => Ok(g),
                other => {
                    // Nieuw model/dimensie: nieuwe generatie (herindexeren via `reindex`).
                    tx.execute("UPDATE index_generations SET active = 0", [])?;
                    let g: i64 = tx.query_row("SELECT COALESCE(MAX(generation), 0) + 1 FROM index_generations", [], |r| r.get(0))?;
                    tx.execute(
                        "INSERT INTO index_generations(generation, model, dims, normalized, active, created_at) VALUES (?1, ?2, ?3, 1, 1, ?4)",
                        params![g, model, dims as i64, now()],
                    )?;
                    if other.is_some() {
                        tracing::warn!(generation = g, "nieuwe indexgeneratie (ander embeddingmodel/dimensie); herindexeren nodig");
                    }
                    Ok(g)
                }
            }
        })?;
        let me = Self {
            log,
            embedder,
            llm,
            index: RwLock::new(Arc::new(FlatI8Index::new(dims))),
            meta: RwLock::new(HashMap::new()),
            generation: AtomicI64::new(generation),
            cfg,
            cache_gen: AtomicU64::new(0),
        };
        me.load_index()?;
        Ok(me)
    }

    fn db(&self) -> &Arc<Db> {
        self.log.db()
    }

    pub fn config(&self) -> &MemConfig {
        &self.cfg
    }

    /// Het embeddingmodel van dit geheugen (bv. voor retrieval-eval met hetzelfde model).
    pub fn embedder(&self) -> Arc<dyn Embedder> {
        self.embedder.clone()
    }

    pub fn cache_generation(&self) -> u64 {
        self.cache_gen.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.cache_gen.fetch_add(1, Ordering::Relaxed);
    }

    /// Index + metadata uit SQLite opbouwen.
    pub fn load_index(&self) -> Result<usize> {
        let g = self.generation.load(Ordering::Relaxed);
        let idx: Arc<dyn VectorIndex> = Arc::new(FlatI8Index::new(self.cfg.dims));
        let mut meta = HashMap::new();
        self.db().read(|c| {
            let mut st = c.prepare("SELECT memory_id, scale, data FROM vectors WHERE generation = ?1")?;
            let mut rows = st.query([g])?;
            while let Some(r) = rows.next()? {
                let id: String = r.get(0)?;
                let data: Vec<u8> = r.get(2)?;
                if let Ok(id) = id.parse() {
                    idx.upsert(id, data.into_iter().map(|b| b as i8).collect(), r.get::<_, f64>(1)? as f32);
                }
            }
            let mut st = c.prepare(
                "SELECT m.id, m.kind, m.workspace_id, m.created_at, m.confidence, m.stale, s.uri FROM memories m LEFT JOIN sources s ON s.id = m.source_id",
            )?;
            let mut rows = st.query([])?;
            while let Some(r) = rows.next()? {
                let id: String = r.get(0)?;
                let kind: String = r.get(1)?;
                if let (Ok(id), Some(kind)) = (id.parse(), MemoryKind::parse(&kind)) {
                    meta.insert(
                        id,
                        Meta {
                            kind,
                            workspace: r.get(2)?,
                            created_at: r.get(3)?,
                            confidence: r.get(4)?,
                            stale: r.get::<_, i64>(5)? != 0,
                            source_uri: r.get(6)?,
                        },
                    );
                }
            }
            Ok(())
        })?;
        let n = idx.len();
        *self.index.write() = idx;
        *self.meta.write() = meta;
        self.bump();
        Ok(n)
    }

    pub(crate) fn index(&self) -> Arc<dyn VectorIndex> {
        self.index.read().clone()
    }

    fn ws_allowed(meta: &Meta, ws: &Option<String>) -> bool {
        meta.workspace.is_none() || meta.workspace == *ws
    }

    // ---------------- schrijven ----------------

    /// Schrijfpipeline (W05.04): policy → chunken → contextbeschrijving → embedden → dedup/merge →
    /// conflictcheck → entiteiten/claims/graph → samenvatting → opslaan (één transactie) → index.
    pub async fn write(&self, m: NewMemory) -> Result<WriteOutcome> {
        let trace = TraceId::new();
        let (imp, feats) = policy::importance(&m.content, m.kind);
        let importance = m.importance.unwrap_or(imp);
        let store = m.force || importance >= self.cfg.min_importance;
        // Beslissing loggen (trainingsdata voor de importance scorer, W11).
        let _ = self.log.log_decision(
            trace,
            None,
            DecisionRecord {
                decision_id: EventId::new(),
                decider: "memory.write_policy".into(),
                decider_version: "heuristiek-v1".into(),
                features: serde_json::to_value(&feats)?,
                choice: serde_json::json!(if store { "store" } else { "skip" }),
                alternatives: vec![],
                confidence: Some(importance),
                outcome: None,
            },
        );
        if !store {
            return Ok(WriteOutcome::Skipped { reason: "onder de schrijfdrempel".into(), importance });
        }
        if m.content.trim().is_empty() {
            return Err(MemError::Invalid("lege herinnering".into()));
        }
        if let Some(ws) = &m.workspace {
            self.ensure_workspace(ws)?;
        }
        let source_id = match &m.source_uri {
            Some(uri) => Some(self.upsert_source(
                uri,
                m.source_hash.as_deref().unwrap_or(&format!("{:016x}", fnv1a64(m.content.as_bytes()))),
            )?),
            None => None,
        };
        let chunks = chunk(&m.content, self.cfg.chunk_chars);
        let mut ids = Vec::new();
        let mut conflicts = 0;
        for c in chunks {
            let hash =
                format!("{:016x}", fnv1a64(format!("{}\0{}", m.workspace.as_deref().unwrap_or(""), c.text).as_bytes()));
            // Exact duplicaat?
            let exact: Option<String> = self.db().read(|cn| {
                Ok(cn.query_row("SELECT id FROM memories WHERE content_hash = ?1", [&hash], |r| r.get(0)).optional()?)
            })?;
            if let Some(id) = exact {
                let id: MemoryId = id.parse().map_err(|_| MemError::Invalid("id".into()))?;
                self.merge_into(id, m.confidence)?;
                if ids.is_empty() && m.content.len() <= self.cfg.chunk_chars {
                    return Ok(WriteOutcome::Merged { into: id });
                }
                ids.push(id);
                continue;
            }
            let context = self.llm.describe_context(&m.title, &c.text).await.unwrap_or_default();
            let emb_text = if context.is_empty() { c.text.clone() } else { format!("{context}\n{}", c.text) };
            let vec = self
                .embedder
                .embed(&[emb_text])
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| MemError::Embed("geen vector".into()))?;
            let vec = crate::vector::normalize(vec, self.cfg.dims);
            // Bijna-duplicaat → samenvoegen.
            let ws = m.workspace.clone();
            let meta = self.meta.read().clone();
            let near = self.index().search(&vec, 5, &|id| meta.get(id).is_some_and(|x| Self::ws_allowed(x, &ws)));
            if let Some((nid, sim)) = near.first().copied()
                && sim >= self.cfg.dedup_threshold
            {
                self.merge_into(nid, m.confidence)?;
                if m.content.len() <= self.cfg.chunk_chars {
                    return Ok(WriteOutcome::Merged { into: nid });
                }
                ids.push(nid);
                continue;
            }
            let extraction = self.llm.extract(&c.text).await.unwrap_or_default();
            let summary = if c.text.len() > self.cfg.summarize_over_chars {
                self.llm.summarize(std::slice::from_ref(&c.text), 60).await.unwrap_or_default()
            } else {
                String::new()
            };
            // Kandidaten voor tegenstrijdigheid.
            let suspects: Vec<MemoryId> =
                near.iter().filter(|(_, s)| *s >= self.cfg.conflict_threshold).map(|(i, _)| *i).collect();
            let id = MemoryId::new();
            let (q, scale) = quantize(&vec);
            let g = self.generation.load(Ordering::Relaxed);
            let salience = policy::salience(&c.text);
            let locator = match (&m.locator, c.locator.is_empty()) {
                (Some(l), true) => Some(l.clone()),
                (Some(l), false) => Some(format!("{l} · {}", c.locator)),
                (None, false) => Some(c.locator.clone()),
                (None, true) => None,
            };
            let n_conf = self.log.transact(|tx, out| {
                let t = now();
                tx.execute(
                    "INSERT INTO memories(id, kind, workspace_id, session_id, content, context, summary, title, source_id, locator,
                        provenance, importance, salience, confidence, valid_from, valid_to, event_time, content_hash, tags,
                        created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?20)",
                    params![
                        id.to_string(),
                        m.kind.as_str(),
                        m.workspace,
                        m.session_id.map(|s| s.to_string()),
                        c.text,
                        context,
                        summary,
                        m.title,
                        source_id,
                        locator,
                        serde_json::to_string(&m.provenance)?,
                        importance,
                        salience,
                        m.confidence.unwrap_or(0.7),
                        m.valid_from,
                        m.valid_to,
                        m.event_time,
                        hash,
                        serde_json::to_string(&m.tags)?,
                        t
                    ],
                )
                .map_err(vortex_core::CoreError::from)?;
                tx.execute(
                    "INSERT INTO vectors(memory_id, generation, scale, data) VALUES (?1, ?2, ?3, ?4)",
                    params![id.to_string(), g, scale as f64, q.iter().map(|b| *b as u8).collect::<Vec<u8>>()],
                )?;
                Self::store_graph(tx, id, &extraction).map_err(|e| vortex_core::CoreError::Config(e.to_string()))?;
                for d in &m.derived_from {
                    tx.execute("INSERT OR IGNORE INTO derivations(derived_id, from_id) VALUES (?1, ?2)", params![id.to_string(), d.to_string()])?;
                }
                if let Some(sid) = &source_id {
                    tx.execute("INSERT OR IGNORE INTO derivations(derived_id, from_id) VALUES (?1, ?2)", params![id.to_string(), sid])?;
                }
                let n = Self::detect_conflicts(tx, out, id, &extraction, &suspects, trace).map_err(|e| vortex_core::CoreError::Config(e.to_string()))?;
                out.emit(
                    tx,
                    Event::new(
                        EventKind::MemoryWritten,
                        trace,
                        // Geen inhoud in het event: vergeten moet echt vergeten zijn.
                        serde_json::json!({"memory_id": id, "kind": m.kind.as_str(), "workspace": m.workspace,
                            "importance": importance, "source": m.source_uri, "conflicts": n}),
                    )
                    .with_provenance(m.provenance.clone()),
                )?;
                Ok(n)
            })?;
            conflicts += n_conf;
            self.index().upsert(id, q, scale);
            self.meta.write().insert(
                id,
                Meta {
                    kind: m.kind,
                    workspace: m.workspace.clone(),
                    created_at: now(),
                    confidence: m.confidence.unwrap_or(0.7),
                    stale: false,
                    source_uri: m.source_uri.clone(),
                },
            );
            ids.push(id);
        }
        self.bump();
        Ok(WriteOutcome::Stored { ids, conflicts })
    }

    fn store_graph(tx: &Transaction<'_>, id: MemoryId, x: &Extraction) -> Result<()> {
        let mut ents: Vec<String> = Vec::new();
        let upsert = |name: &str, ty: &str| -> Result<String> {
            let norm = norm_entity(name);
            let eid: Option<String> =
                tx.query_row("SELECT id FROM entities WHERE norm = ?1", [&norm], |r| r.get(0)).optional()?;
            let eid = match eid {
                Some(e) => e,
                None => {
                    let e = format!("ent_{:016x}", fnv1a64(norm.as_bytes()));
                    tx.execute(
                        "INSERT INTO entities(id, name, norm, type) VALUES (?1, ?2, ?3, ?4)",
                        params![e, name.trim(), norm, ty],
                    )?;
                    e
                }
            };
            tx.execute(
                "INSERT OR IGNORE INTO memory_entities(memory_id, entity_id) VALUES (?1, ?2)",
                params![id.to_string(), eid],
            )?;
            Ok(eid)
        };
        for e in &x.entities {
            if norm_entity(&e.name).len() > 1 {
                ents.push(upsert(&e.name, &e.r#type)?);
            }
        }
        for c in &x.claims {
            let s = upsert(&c.subject, "thing")?;
            let o = upsert(&c.object, "thing")?;
            tx.execute(
                "INSERT INTO claims(id, memory_id, subject, predicate, object, polarity, value, unit) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![EventId::new().to_string(), id.to_string(), norm_entity(&c.subject), c.predicate, norm_entity(&c.object), c.polarity as i64, c.value, c.unit],
            )?;
            tx.execute(
                "INSERT INTO edges(id, src, rel, dst, memory_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![EventId::new().to_string(), s, c.predicate, o, id.to_string(), now()],
            )?;
        }
        // Samen genoemde entiteiten zijn gerelateerd.
        for w in ents.windows(2) {
            tx.execute(
                "INSERT INTO edges(id, src, rel, dst, memory_id, created_at) VALUES (?1, ?2, 'related_to', ?3, ?4, ?5)",
                params![EventId::new().to_string(), w[0], w[1], id.to_string(), now()],
            )?;
        }
        Ok(())
    }

    /// Zelfde onderwerp + predicaat, ander object/polariteit/waarde → conflict.
    fn detect_conflicts(
        tx: &Transaction<'_>,
        out: &mut vortex_core::events::Outbox,
        id: MemoryId,
        x: &Extraction,
        suspects: &[MemoryId],
        trace: TraceId,
    ) -> Result<usize> {
        let mut n = 0;
        for c in &x.claims {
            let mut st = tx.prepare("SELECT memory_id, object, polarity, value FROM claims WHERE subject = ?1 AND predicate = ?2 AND memory_id != ?3")?;
            let rows: Vec<(String, String, i64, Option<f64>)> = st
                .query_map(params![norm_entity(&c.subject), c.predicate, id.to_string()], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            for (other, obj, pol, val) in rows {
                let differs = if c.value.is_some() && val.is_some() {
                    (c.value.unwrap_or(0.0) - val.unwrap_or(0.0)).abs() > 1e-9
                } else {
                    norm_entity(&c.object) != obj || i64::from(c.polarity) != pol
                };
                let same_polarity_same_obj = norm_entity(&c.object) == obj && i64::from(c.polarity) == pol;
                if !differs || same_polarity_same_obj && c.value.is_none() {
                    continue;
                }
                // Alleen "is/woont in/werkt bij/kost/weegt/heeft-aantal": predicaten met één waarheid.
                let single_valued = matches!(c.predicate.as_str(), "is" | "woont in" | "werkt bij" | "kost" | "weegt")
                    || c.value.is_some();
                if !single_valued && i64::from(c.polarity) == pol {
                    continue;
                }
                let other_id: MemoryId = other.parse().map_err(|_| MemError::Invalid("id".into()))?;
                let exists: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM conflicts WHERE (a_memory = ?1 AND b_memory = ?2) OR (a_memory = ?2 AND b_memory = ?1)",
                    params![other, id.to_string()],
                    |r| r.get(0),
                )?;
                if exists > 0 {
                    continue;
                }
                let reason = format!("{} {}: '{}' vs '{}'", c.subject, c.predicate, obj, c.object);
                let cid = format!("cfl_{}", EventId::new().to_string().trim_start_matches("evt_"));
                tx.execute(
                    "INSERT INTO conflicts(id, a_memory, b_memory, reason, status, created_at) VALUES (?1, ?2, ?3, ?4, 'open', ?5)",
                    params![cid, other, id.to_string(), reason, now()],
                )?;
                out.emit(
                    tx,
                    Event::new(
                        EventKind::ConflictDetected,
                        trace,
                        serde_json::json!({"conflict": cid, "a": other_id, "b": id, "reason": reason, "near": suspects.contains(&other_id)}),
                    ),
                )
                .map_err(MemError::Core)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn merge_into(&self, id: MemoryId, confidence: Option<f64>) -> Result<()> {
        self.log.transact(|tx, out| {
            tx.execute(
                "UPDATE memories SET merged_count = merged_count + 1, confidence = MIN(1.0, MAX(confidence, COALESCE(?2, confidence)) + 0.05),
                    updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), confidence, now()],
            )?;
            out.emit(tx, Event::new(EventKind::MemoryMerged, TraceId::new(), serde_json::json!({"memory_id": id})))?;
            Ok(())
        })?;
        self.bump();
        Ok(())
    }

    fn ensure_workspace(&self, name: &str) -> Result<()> {
        self.db().write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO workspaces(id, name, created_at) VALUES (?1, ?1, ?2)",
                params![name, now()],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Bron registreren. Gewijzigde inhoud (andere hash) → alle afgeleiden worden verouderd (W05.10).
    pub fn upsert_source(&self, uri: &str, hash: &str) -> Result<String> {
        let (id, changed) = self.db().write(|tx| {
            let cur: Option<(String, String)> =
                tx.query_row("SELECT id, content_hash FROM sources WHERE uri = ?1", [uri], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            Ok(match cur {
                Some((id, h)) if h == hash => (id, false),
                Some((id, _)) => {
                    tx.execute("UPDATE sources SET content_hash = ?2, updated_at = ?3 WHERE id = ?1", params![id, hash, now()])?;
                    (id, true)
                }
                None => {
                    let id = format!("src_{:016x}", fnv1a64(uri.as_bytes()));
                    tx.execute(
                        "INSERT INTO sources(id, uri, kind, content_hash, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                        params![id, uri, uri.split(':').next().unwrap_or("file"), hash, now()],
                    )?;
                    (id, false)
                }
            })
        })?;
        if changed {
            self.mark_derived_stale(&id, "bron gewijzigd")?;
        }
        Ok(id)
    }

    /// Alles wat (transitief) van `from` is afgeleid op `stale` zetten.
    pub fn mark_derived_stale(&self, from: &str, reason: &str) -> Result<Vec<MemoryId>> {
        let ids = self.derived_closure(from)?;
        if ids.is_empty() {
            return Ok(ids);
        }
        self.log.transact(|tx, out| {
            for id in &ids {
                tx.execute(
                    "UPDATE memories SET stale = 1, updated_at = ?2 WHERE id = ?1",
                    params![id.to_string(), now()],
                )?;
            }
            out.emit(
                tx,
                Event::new(
                    EventKind::MemoryStale,
                    TraceId::new(),
                    serde_json::json!({"from": from, "memories": ids, "reason": reason}),
                ),
            )?;
            Ok(())
        })?;
        {
            let mut meta = self.meta.write();
            for id in &ids {
                if let Some(m) = meta.get_mut(id) {
                    m.stale = true;
                }
            }
        }
        self.bump();
        Ok(ids)
    }

    fn derived_closure(&self, from: &str) -> Result<Vec<MemoryId>> {
        self.db()
            .read(|c| {
                let mut seen: HashSet<String> = HashSet::new();
                let mut todo = vec![from.to_string()];
                let mut st = c.prepare("SELECT derived_id FROM derivations WHERE from_id = ?1")?;
                while let Some(x) = todo.pop() {
                    let kids: Vec<String> = st.query_map([&x], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                    for k in kids {
                        if seen.insert(k.clone()) {
                            todo.push(k);
                        }
                    }
                }
                Ok(seen.into_iter().filter_map(|s| s.parse().ok()).collect())
            })
            .map_err(Into::into)
    }

    // ---------------- lezen / bewerken ----------------

    pub fn get(&self, id: MemoryId) -> Result<Memory> {
        self.db()
            .read(|c| {
                Ok(c.query_row(&format!("{} WHERE m.id = ?1", mem_select()), [id.to_string()], row_mem).optional()?)
            })?
            .ok_or_else(|| MemError::NotFound(id.to_string()))
    }

    /// Lijst met filters (Memory-pagina). `text` = FTS-zoekterm (optioneel).
    pub fn list(&self, f: &Filters, text: Option<&str>, limit: usize, offset: usize) -> Result<Vec<Memory>> {
        let mut sql = format!("{} WHERE 1=1", mem_select());
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) {
            sql.push_str(" AND m.rowid IN (SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?)");
            args.push(crate::retrieve::fts_query(t).into());
        }
        Self::filter_sql(f, &mut sql, &mut args);
        sql.push_str(" ORDER BY m.updated_at DESC LIMIT ? OFFSET ?");
        args.push((limit as i64).into());
        args.push((offset as i64).into());
        Ok(self.db().read(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), row_mem)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?)
    }

    pub(crate) fn filter_sql(f: &Filters, sql: &mut String, args: &mut Vec<rusqlite::types::Value>) {
        if !f.kinds.is_empty() {
            sql.push_str(&format!(" AND m.kind IN ({})", vec!["?"; f.kinds.len()].join(",")));
            args.extend(f.kinds.iter().map(|k| k.as_str().to_string().into()));
        }
        match &f.workspace {
            Some(w) => {
                sql.push_str(" AND (m.workspace_id IS NULL OR m.workspace_id = ?)");
                args.push(w.clone().into());
            }
            None => sql.push_str(" AND m.workspace_id IS NULL"),
        }
        if let Some(s) = &f.since {
            sql.push_str(" AND COALESCE(m.event_time, m.created_at) >= ?");
            args.push(s.clone().into());
        }
        if let Some(u) = &f.until {
            sql.push_str(" AND COALESCE(m.event_time, m.created_at) <= ?");
            args.push(u.clone().into());
        }
        if let Some(c) = f.min_confidence {
            sql.push_str(" AND m.confidence >= ?");
            args.push(c.into());
        }
        if let Some(p) = &f.source_prefix {
            sql.push_str(" AND s.uri LIKE ?");
            args.push(format!("{p}%").into());
        }
        if !f.include_stale {
            sql.push_str(" AND m.stale = 0");
        }
    }

    /// Bewerken: nieuwe versie (oude in `memory_versions`), opnieuw embedden en extraheren.
    pub async fn update(&self, id: MemoryId, content: &str, reason: &str) -> Result<Memory> {
        let old = self.get(id)?;
        let vec = self
            .embedder
            .embed(&[format!("{}\n{content}", old.context)])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| MemError::Embed("geen vector".into()))?;
        let vec = crate::vector::normalize(vec, self.cfg.dims);
        let (q, scale) = quantize(&vec);
        let x = self.llm.extract(content).await.unwrap_or_default();
        let g = self.generation.load(Ordering::Relaxed);
        let hash =
            format!("{:016x}", fnv1a64(format!("{}\0{}", old.workspace.as_deref().unwrap_or(""), content).as_bytes()));
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO memory_versions(memory_id, version, content, reason, changed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id.to_string(), old.version, old.content, reason, now()],
            )?;
            tx.execute(
                "UPDATE memories SET content = ?2, version = version + 1, content_hash = ?3, stale = 0, updated_at = ?4 WHERE id = ?1",
                params![id.to_string(), content, hash, now()],
            )?;
            tx.execute("DELETE FROM claims WHERE memory_id = ?1", [id.to_string()])?;
            tx.execute("DELETE FROM edges WHERE memory_id = ?1", [id.to_string()])?;
            tx.execute("DELETE FROM memory_entities WHERE memory_id = ?1", [id.to_string()])?;
            tx.execute(
                "INSERT OR REPLACE INTO vectors(memory_id, generation, scale, data) VALUES (?1, ?2, ?3, ?4)",
                params![id.to_string(), g, scale as f64, q.iter().map(|b| *b as u8).collect::<Vec<u8>>()],
            )?;
            Self::store_graph(tx, id, &x).map_err(|e| vortex_core::CoreError::Config(e.to_string()))?;
            out.emit(tx, Event::new(EventKind::MemoryUpdated, TraceId::new(), serde_json::json!({"memory_id": id, "version": old.version + 1, "reason": reason})))?;
            Ok(())
        })?;
        self.index().upsert(id, q, scale);
        if let Some(m) = self.meta.write().get_mut(&id) {
            m.stale = false;
        }
        // Afgeleiden (samenvattingen) kloppen niet meer.
        self.mark_derived_stale(&id.to_string(), "bron-herinnering bewerkt")?;
        self.bump();
        self.get(id)
    }

    pub fn versions(&self, id: MemoryId) -> Result<Vec<(i64, String, String, String)>> {
        Ok(self.db().read(|c| {
            let mut st = c.prepare("SELECT version, content, reason, changed_at FROM memory_versions WHERE memory_id = ?1 ORDER BY version")?;
            let rows = st.query_map([id.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?)
    }

    /// "Vergeet dit" met deletion propagation (W05.09): herinnering + alles wat ervan is afgeleid
    /// (samenvattingen), vectoren, FTS, graph-koppelingen, claims, versies, conflicten, wees-
    /// entiteiten en caches. Geeft alle verwijderde ID's terug.
    pub fn forget(&self, id: MemoryId) -> Result<Vec<MemoryId>> {
        let mut all = vec![id];
        all.extend(self.derived_closure(&id.to_string())?);
        let all: Vec<MemoryId> = all.into_iter().collect::<HashSet<_>>().into_iter().collect();
        let n = self.log.transact(|tx, out| {
            let mut n = 0;
            for m in &all {
                let s = m.to_string();
                for sql in [
                    "DELETE FROM vectors WHERE memory_id = ?1",
                    "DELETE FROM claims WHERE memory_id = ?1",
                    "DELETE FROM edges WHERE memory_id = ?1",
                    "DELETE FROM memory_entities WHERE memory_id = ?1",
                    "DELETE FROM memory_versions WHERE memory_id = ?1",
                    "DELETE FROM derivations WHERE derived_id = ?1 OR from_id = ?1",
                    "DELETE FROM conflicts WHERE a_memory = ?1 OR b_memory = ?1",
                ] {
                    tx.execute(sql, [&s])?;
                }
                n += tx.execute("DELETE FROM memories WHERE id = ?1", [&s])?;
            }
            tx.execute("DELETE FROM entities WHERE id NOT IN (SELECT entity_id FROM memory_entities)", [])?;
            tx.execute(
                "DELETE FROM edges WHERE src NOT IN (SELECT id FROM entities) OR dst NOT IN (SELECT id FROM entities)",
                [],
            )?;
            out.emit(
                tx,
                Event::new(
                    EventKind::MemoryForgotten,
                    TraceId::new(),
                    serde_json::json!({"memories": all, "requested": id}),
                ),
            )?;
            Ok(n)
        })?;
        if n == 0 {
            return Err(MemError::NotFound(id.to_string()));
        }
        let idx = self.index();
        let mut meta = self.meta.write();
        for m in &all {
            idx.remove(m);
            meta.remove(m);
        }
        drop(meta);
        self.bump();
        Ok(all)
    }

    // ---------------- conflicten ----------------

    pub fn conflicts(&self, status: Option<&str>) -> Result<Vec<Conflict>> {
        Ok(self.db().read(|c| {
            let mut st = c.prepare(
                "SELECT id, a_memory, b_memory, reason, status, resolution, created_at FROM conflicts WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC",
            )?;
            let rows = st.query_map([status], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, a, b, reason, status, resolution, created_at) = row?;
                if let (Ok(a), Ok(b)) = (a.parse(), b.parse()) {
                    out.push(Conflict { id, a, b, reason, status, resolution, created_at });
                }
            }
            Ok(out)
        })?)
    }

    /// Conflict afhandelen: `winner` blijft; de ander wordt historie (valid_to = nu, lagere
    /// confidence) — of status "disputed" als niet te beslissen. (Het conflict-team in W09
    /// gebruikt dit.)
    pub fn resolve_conflict(&self, conflict_id: &str, winner: Option<MemoryId>, note: &str) -> Result<()> {
        let c = self
            .conflicts(None)?
            .into_iter()
            .find(|c| c.id == conflict_id)
            .ok_or_else(|| MemError::NotFound(conflict_id.into()))?;
        self.log.transact(|tx, out| {
            match winner {
                Some(w) => {
                    let loser = if w == c.a { c.b } else { c.a };
                    tx.execute(
                        "UPDATE memories SET valid_to = ?2, confidence = confidence * 0.5, updated_at = ?2 WHERE id = ?1",
                        params![loser.to_string(), now()],
                    )?;
                    tx.execute(
                        "UPDATE conflicts SET status = 'resolved', resolution = ?2, resolved_at = ?3 WHERE id = ?1",
                        params![conflict_id, format!("winnaar {w}: {note}"), now()],
                    )?;
                }
                None => {
                    tx.execute(
                        "UPDATE conflicts SET status = 'disputed', resolution = ?2, resolved_at = ?3 WHERE id = ?1",
                        params![conflict_id, note, now()],
                    )?;
                }
            }
            out.emit(tx, Event::new(EventKind::ConflictResolved, TraceId::new(), serde_json::json!({"conflict": conflict_id, "winner": winner, "note": note})))?;
            Ok(())
        })?;
        self.bump();
        Ok(())
    }

    // ---------------- werkruimtes ----------------

    pub fn workspaces(&self) -> Result<Vec<String>> {
        Ok(self.db().read(|c| {
            let mut st = c.prepare("SELECT name FROM workspaces ORDER BY name")?;
            let rows = st.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
        })?)
    }

    // ---------------- statistiek ----------------

    pub fn stats(&self) -> Result<serde_json::Value> {
        Ok(self.db().read(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
            let e: i64 = c.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
            let ed: i64 = c.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
            let cf: i64 = c.query_row("SELECT COUNT(*) FROM conflicts WHERE status = 'open'", [], |r| r.get(0))?;
            let mut st = c.prepare("SELECT kind, COUNT(*) FROM memories GROUP BY kind")?;
            let by: HashMap<String, i64> =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            Ok(serde_json::json!({"memories": n, "entities": e, "edges": ed, "open_conflicts": cf, "by_kind": by}))
        })?)
    }

    // ---------------- export / import / backup ----------------

    /// Exporteer alles (of één werkruimte) als JSONL.
    pub fn export(&self, w: &mut dyn Write, workspace: Option<&str>) -> Result<usize> {
        let mems = self.db().read(|c| {
            let mut st = c.prepare(&format!(
                "{} WHERE (?1 IS NULL OR m.workspace_id = ?1) ORDER BY m.created_at",
                mem_select()
            ))?;
            let rows = st.query_map([workspace], row_mem)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut n = 0;
        for m in mems {
            let (claims, entities) = self.db().read(|c| {
                let mut st = c.prepare("SELECT subject, predicate, object, polarity, value, unit FROM claims WHERE memory_id = ?1")?;
                let claims = st
                    .query_map([m.id.to_string()], |r| {
                        Ok(crate::llm::ExtractedClaim {
                            subject: r.get(0)?,
                            predicate: r.get(1)?,
                            object: r.get(2)?,
                            polarity: r.get::<_, i64>(3)? as i8,
                            value: r.get(4)?,
                            unit: r.get(5)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut st = c.prepare("SELECT e.name FROM entities e JOIN memory_entities me ON me.entity_id = e.id WHERE me.memory_id = ?1")?;
                let ents = st.query_map([m.id.to_string()], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
                Ok((claims, ents))
            })?;
            serde_json::to_writer(&mut *w, &ExportRow { memory: m, claims, entities })?;
            w.write_all(b"\n")?;
            n += 1;
        }
        Ok(n)
    }

    /// Importeer JSONL (via de normale schrijfpipeline, dus dedup/conflicten werken mee).
    pub async fn import(&self, r: &mut (dyn BufRead + Send)) -> Result<usize> {
        let mut n = 0;
        for line in r.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let row: ExportRow = serde_json::from_str(&line)?;
            let m = row.memory;
            let mut nm = NewMemory::new(m.kind, m.content, m.provenance);
            nm.title = m.title;
            nm.workspace = m.workspace;
            nm.source_uri = m.source_uri;
            nm.locator = m.locator;
            nm.importance = Some(m.importance);
            nm.confidence = Some(m.confidence);
            nm.tags = m.tags;
            nm.event_time = m.event_time;
            nm.force = true;
            if !matches!(self.write(nm).await?, WriteOutcome::Skipped { .. }) {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Volledige backup van de database (online, consistent) naar `dest` (bv. op de HDD).
    pub fn backup(&self, dest: &Path) -> Result<()> {
        let src = self
            .db()
            .path()
            .ok_or_else(|| MemError::Invalid("in-memory database kan niet geback-upt worden".into()))?;
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        self.db().checkpoint_wal()?;
        let from = rusqlite::Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut to = rusqlite::Connection::open(dest)?;
        let b = rusqlite::backup::Backup::new(&from, &mut to)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        Ok(())
    }

    /// Backup terugzetten: kopieer `backup` over het databasebestand (kern moet gestopt zijn).
    pub fn restore(backup: &Path, db_path: &Path) -> Result<()> {
        let from = rusqlite::Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut to = rusqlite::Connection::open(db_path)?;
        let b = rusqlite::backup::Backup::new(&from, &mut to)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        Ok(())
    }

    /// Schrijven, of — als het embeddingmodel (nog) niet bereikbaar is — persistent in de wachtrij.
    pub async fn write_or_defer(&self, m: NewMemory) -> Result<Option<WriteOutcome>> {
        match self.write(m.clone()).await {
            Ok(o) => Ok(Some(o)),
            Err(MemError::Embed(e)) => {
                tracing::warn!(error = %e, "embedding niet beschikbaar; herinnering in wachtrij");
                self.db().write(|tx| {
                    tx.execute(
                        "INSERT INTO memory_pending(payload, last_error, created_at) VALUES (?1, ?2, ?3)",
                        params![serde_json::to_string(&m)?, e, now()],
                    )?;
                    Ok(())
                })?;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Wachtrij opnieuw proberen. Geeft (gelukt, nog wachtend) terug.
    pub async fn retry_pending(&self, max: usize) -> Result<(usize, usize)> {
        let rows: Vec<(i64, String)> = self.db().read(|c| {
            let mut st = c.prepare("SELECT id, payload FROM memory_pending ORDER BY id LIMIT ?1")?;
            let rows = st.query_map([max as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut ok = 0;
        for (id, payload) in rows {
            let m: NewMemory = serde_json::from_str(&payload)?;
            match self.write(m).await {
                Ok(_) => {
                    self.db().write(|tx| Ok(tx.execute("DELETE FROM memory_pending WHERE id = ?1", [id])?))?;
                    ok += 1;
                }
                Err(MemError::Embed(e)) => {
                    self.db().write(|tx| {
                        Ok(tx.execute(
                            "UPDATE memory_pending SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1",
                            params![id, e],
                        )?)
                    })?;
                    break; // model nog weg: later verder
                }
                Err(e) => {
                    tracing::error!(error = %e, "herinnering uit wachtrij ongeldig; verwijderd");
                    self.db().write(|tx| Ok(tx.execute("DELETE FROM memory_pending WHERE id = ?1", [id])?))?;
                }
            }
        }
        let left: i64 =
            self.db().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM memory_pending", [], |r| r.get(0))?))?;
        Ok((ok, left as usize))
    }

    /// Toolresultaat → geheugen via de policy (W05.08). Korte/lege resultaten worden niet bewaard.
    // VX:W05.08
    pub async fn remember_tool_result(
        &self,
        tool: &str,
        args: &serde_json::Value,
        result: &str,
        workspace: Option<String>,
        session: Option<vortex_types::SessionId>,
    ) -> Result<Option<WriteOutcome>> {
        let text: String = result.chars().take(8000).collect();
        let mut m = NewMemory::new(
            MemoryKind::Episodic,
            format!("Tool {tool} ({}):\n{text}", args.to_string().chars().take(300).collect::<String>()),
            Provenance::with_origin(vortex_types::SourceKind::Tool, tool),
        );
        m.title = format!("Toolresultaat {tool}");
        m.workspace = workspace;
        m.session_id = session;
        m.tags = vec!["tool".into(), tool.into()];
        // Toolresultaten zijn feitelijke waarnemingen: iets hoger dan episodisch standaard.
        let (imp, _) = policy::importance(&text, MemoryKind::Episodic);
        m.importance = Some((imp + 0.1).min(1.0));
        self.write_or_defer(m).await
    }

    /// Herinnering als gebruikt markeren (recency/usefulness).
    pub(crate) fn touch(&self, ids: &[MemoryId]) -> Result<()> {
        self.db().write(|tx| {
            for id in ids {
                tx.execute(
                    "UPDATE memories SET access_count = access_count + 1, last_access = ?2 WHERE id = ?1",
                    params![id.to_string(), now()],
                )?;
            }
            Ok(())
        })?;
        Ok(())
    }

    pub(crate) fn meta_snapshot(&self) -> HashMap<MemoryId, Meta> {
        self.meta.read().clone()
    }
}
