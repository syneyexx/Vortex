//! Dromen in idle (W12.03): consolidatie — bijna-duplicaten samenvoegen, hiërarchische
//! tijd-samenvattingen (dag → week → maand), decay/pruning, reflectie, graph-redeneren
//! (+ consistentie → conflicten), vertrouwen bijwerken op onafhankelijke steun/tegenspraak.
// VX:W12.03

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventId, EventKind, MemoryId, Provenance, SourceKind, TraceId};

use crate::error::Result;
use crate::graph::{ReasonReport, Rule};
use crate::store::MemoryStore;
use crate::types::{Filters, MemoryKind, NewMemory, WriteOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamOptions {
    /// Cosinus vanaf waar twee herinneringen samengevoegd worden (lager dan bij schrijven).
    pub merge_threshold: f32,
    pub max_merge: usize,
    pub reflect: bool,
    #[serde(skip)]
    pub rules: Vec<Rule>,
}

impl Default for DreamOptions {
    fn default() -> Self {
        Self { merge_threshold: 0.9, max_merge: 500, reflect: true, rules: crate::graph::default_rules() }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DreamReport {
    pub merged: usize,
    pub day_summaries: usize,
    pub week_summaries: usize,
    pub month_summaries: usize,
    pub reflections: usize,
    pub pruned: usize,
    pub promoted: usize,
    pub reasoning: ReasonReport,
    pub confidence_up: usize,
    pub confidence_down: usize,
    pub open_conflicts: usize,
    pub ms: u64,
}

/// (polariteit, herinnering, bron)
type ClaimRow = (i64, String, Option<String>);

const MERGE_KINDS: &[MemoryKind] =
    &[MemoryKind::Semantic, MemoryKind::User, MemoryKind::Project, MemoryKind::World, MemoryKind::LongTerm];

fn date_of(m: &crate::types::Memory) -> String {
    m.event_time.clone().unwrap_or_else(|| m.created_at.clone()).chars().take(10).collect()
}

fn iso_week(d: &str) -> Option<String> {
    let dt = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    let w = chrono::Datelike::iso_week(&dt);
    Some(format!("{}-W{:02}", w.year(), w.week()))
}

impl MemoryStore {
    /// Twee herinneringen samenvoegen: `drop` wordt verouderd en afgeleid van `keep`.
    pub fn merge_pair(&self, keep: MemoryId, drop: MemoryId) -> Result<()> {
        self.log.transact(|tx, out| {
            tx.execute("UPDATE memories SET stale = 1, updated_at = ?2 WHERE id = ?1", params![drop.to_string(), vortex_types::now().to_rfc3339()])?;
            tx.execute(
                "UPDATE memories SET merged_count = merged_count + 1, confidence = MIN(0.98, confidence + 0.03), updated_at = ?2 WHERE id = ?1",
                params![keep.to_string(), vortex_types::now().to_rfc3339()],
            )?;
            tx.execute("INSERT OR IGNORE INTO derivations(derived_id, from_id) VALUES (?1, ?2)", params![drop.to_string(), keep.to_string()])?;
            out.emit(tx, Event::new(EventKind::MemoryMerged, TraceId::new(), serde_json::json!({"into": keep, "from": drop, "by": "dream"})))?;
            Ok(())
        })?;
        if let Some(x) = self.meta.write().get_mut(&drop) {
            x.stale = true;
        }
        self.index().remove(&drop);
        self.bump();
        Ok(())
    }

    async fn summary_for(
        &self,
        uri: &str,
        title: &str,
        texts: Vec<String>,
        from: Vec<MemoryId>,
        tag: &str,
        period: &str,
    ) -> Result<bool> {
        if self.source_hash(uri)?.is_some() || texts.is_empty() {
            return Ok(false);
        }
        let s = self.llm.summarize(&texts, 120).await?;
        if s.trim().is_empty() {
            return Ok(false);
        }
        let mut nm =
            NewMemory::new(MemoryKind::Summary, s, Provenance::with_origin(SourceKind::Memory, uri.to_string()));
        nm.title = title.into();
        nm.source_uri = Some(uri.into());
        nm.source_hash = Some(uri.into());
        nm.derived_from = from;
        nm.tags = vec!["gedroomd".into(), tag.into(), period.into()];
        nm.event_time = Some(format!("{period}T00:00:00Z").chars().take(20).collect());
        nm.force = true;
        Ok(matches!(self.write(nm).await?, WriteOutcome::Stored { .. }))
    }

    pub async fn dream(&self, o: &DreamOptions) -> Result<DreamReport> {
        let t0 = Instant::now();
        let mut rep = DreamReport::default();
        // 1. Bijna-duplicaten samenvoegen.
        let mems = self.list(&Filters { kinds: MERGE_KINDS.to_vec(), ..Default::default() }, None, 5000, 0)?;
        let ids: Vec<MemoryId> = mems.iter().map(|m| m.id).collect();
        let vecs = self.vectors_of(&ids)?;
        let by_id: HashMap<MemoryId, &crate::types::Memory> = mems.iter().map(|m| (m.id, m)).collect();
        let mut gone: HashSet<MemoryId> = HashSet::new();
        for m in &mems {
            if rep.merged >= o.max_merge || gone.contains(&m.id) {
                continue;
            }
            let Some(v) = vecs.get(&m.id) else { continue };
            let meta = self.meta.read().clone();
            let hits = self.index().search(v, 4, &|id| {
                *id != m.id
                    && !gone.contains(id)
                    && meta
                        .get(id)
                        .is_some_and(|x| !x.stale && x.workspace == m.workspace && MERGE_KINDS.contains(&x.kind))
            });
            for (other, sim) in hits {
                if sim < o.merge_threshold {
                    break;
                }
                let Some(om) = by_id.get(&other) else { continue };
                // Bewaar de betrouwbaarste/oudste; gebruikersfeiten nooit tegen een niet-gebruikersfeit wegstrepen.
                let (keep, drop) = if (m.confidence, std::cmp::Reverse(m.created_at.clone()))
                    >= (om.confidence, std::cmp::Reverse(om.created_at.clone()))
                {
                    (m.id, other)
                } else {
                    (other, m.id)
                };
                if by_id.get(&drop).is_some_and(|d| d.kind == MemoryKind::User)
                    && by_id.get(&keep).is_some_and(|k| k.kind != MemoryKind::User)
                {
                    continue;
                }
                self.merge_pair(keep, drop)?;
                gone.insert(drop);
                rep.merged += 1;
                if drop == m.id {
                    break;
                }
            }
        }
        // 2. Hiërarchische tijd-samenvattingen: dag → week → maand (alleen afgesloten periodes).
        let today = vortex_types::now().format("%Y-%m-%d").to_string();
        let eps = self.list(&Filters { kinds: vec![MemoryKind::Episodic], ..Default::default() }, None, 20_000, 0)?;
        let mut days: BTreeMap<String, Vec<&crate::types::Memory>> = BTreeMap::new();
        for e in &eps {
            let d = date_of(e);
            if d < today {
                days.entry(d).or_default().push(e);
            }
        }
        for (d, list) in &days {
            if list.len() >= 2
                && self
                    .summary_for(
                        &format!("dream:dag:{d}"),
                        &format!("Dag {d}"),
                        list.iter().map(|m| m.content.chars().take(800).collect()).collect(),
                        list.iter().map(|m| m.id).collect(),
                        "dag",
                        d,
                    )
                    .await?
            {
                rep.day_summaries += 1;
            }
        }
        let summaries =
            self.list(&Filters { kinds: vec![MemoryKind::Summary], ..Default::default() }, None, 20_000, 0)?;
        let this_week = iso_week(&today).unwrap_or_default();
        let mut weeks: BTreeMap<String, Vec<&crate::types::Memory>> = BTreeMap::new();
        for s in summaries.iter().filter(|s| s.tags.iter().any(|t| t == "dag")) {
            if let Some(w) = iso_week(&date_of(s))
                && w < this_week
            {
                weeks.entry(w).or_default().push(s);
            }
        }
        for (w, list) in &weeks {
            if self
                .summary_for(
                    &format!("dream:week:{w}"),
                    &format!("Week {w}"),
                    list.iter().map(|m| m.content.clone()).collect(),
                    list.iter().map(|m| m.id).collect(),
                    "week",
                    &date_of(list[0]),
                )
                .await?
            {
                rep.week_summaries += 1;
            }
        }
        let this_month: String = today.chars().take(7).collect();
        let mut months: BTreeMap<String, Vec<&crate::types::Memory>> = BTreeMap::new();
        for s in summaries.iter().filter(|s| s.tags.iter().any(|t| t == "dag")) {
            let m: String = date_of(s).chars().take(7).collect();
            if m < this_month {
                months.entry(m).or_default().push(s);
            }
        }
        for (m, list) in &months {
            if list.len() >= 2
                && self
                    .summary_for(
                        &format!("dream:maand:{m}"),
                        &format!("Maand {m}"),
                        list.iter().map(|x| x.content.clone()).collect(),
                        list.iter().map(|x| x.id).collect(),
                        "maand",
                        &format!("{m}-01"),
                    )
                    .await?
            {
                rep.month_summaries += 1;
            }
        }
        // 3. Decay/pruning van kortetermijngeheugen.
        let d = self.decay(24.0, 0.5)?;
        rep.pruned = d.pruned.len();
        rep.promoted = d.promoted.len();
        // 4. Reflectie: inzichten uit recente episodes (laatste 7 dagen) → semantisch geheugen.
        if o.reflect {
            let week_ago = (vortex_types::now() - chrono::Duration::days(7)).to_rfc3339();
            let recent: Vec<&crate::types::Memory> = eps.iter().filter(|e| e.created_at >= week_ago).take(20).collect();
            let uri = format!("dream:reflectie:{today}");
            if recent.len() >= 3 && self.source_hash(&uri)?.is_none() {
                let texts: Vec<String> = std::iter::once(
                    "Welke terugkerende onderwerpen, voorkeuren en lessen zie je in deze gesprekken?".to_string(),
                )
                .chain(recent.iter().map(|m| m.content.chars().take(600).collect()))
                .collect();
                let s = self.llm.summarize(&texts, 100).await?;
                if !s.trim().is_empty() {
                    let mut nm = NewMemory::new(
                        MemoryKind::Semantic,
                        s,
                        Provenance::with_origin(SourceKind::Memory, uri.clone()),
                    );
                    nm.title = format!("Reflectie {today}");
                    nm.source_uri = Some(uri.clone());
                    nm.source_hash = Some(uri);
                    nm.derived_from = recent.iter().map(|m| m.id).collect();
                    nm.tags = vec!["gedroomd".into(), "reflectie".into()];
                    nm.confidence = Some(0.5);
                    nm.force = true;
                    if matches!(self.write(nm).await?, WriteOutcome::Stored { .. }) {
                        rep.reflections += 1;
                    }
                }
            }
        }
        // 5. Graph-redeneren + consistentie (→ conflicten voor het conflict-team).
        rep.reasoning = self.reason(&o.rules)?;
        // 6. Vertrouwen: zelfde claim uit onafhankelijke bronnen ↑, tegengestelde polariteit ↓.
        let rows: Vec<(String, String, String, i64, String, Option<String>)> = self.log.db().read(|c| {
            let mut st = c.prepare(
                "SELECT c.subject, c.predicate, c.object, c.polarity, c.memory_id, m.source_id FROM claims c JOIN memories m ON m.id = c.memory_id WHERE m.stale = 0",
            )?;
            let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?;
        let mut groups: HashMap<(String, String, String), Vec<ClaimRow>> = HashMap::new();
        for (s, p, ob, pol, mid, src) in rows {
            groups.entry((s.to_lowercase(), p.to_lowercase(), ob.to_lowercase())).or_default().push((pol, mid, src));
        }
        let (mut up, mut down) = (Vec::new(), Vec::new());
        for list in groups.values() {
            let pos: Vec<&ClaimRow> = list.iter().filter(|x| x.0 > 0).collect();
            let neg: Vec<&ClaimRow> = list.iter().filter(|x| x.0 <= 0).collect();
            let srcs: HashSet<String> = pos.iter().map(|x| x.2.clone().unwrap_or_else(|| x.1.clone())).collect();
            if srcs.len() >= 2 {
                up.extend(pos.iter().map(|x| x.1.clone()));
            }
            if !pos.is_empty() && !neg.is_empty() {
                down.extend(list.iter().map(|x| x.1.clone()));
            }
        }
        up.sort();
        up.dedup();
        down.sort();
        down.dedup();
        self.log.db().write(|tx| {
            for m in &up {
                tx.execute("UPDATE memories SET confidence = MIN(0.98, confidence + 0.05) WHERE id = ?1", [m])?;
            }
            for m in &down {
                tx.execute("UPDATE memories SET confidence = MAX(0.1, confidence - 0.1) WHERE id = ?1", [m])?;
            }
            Ok(())
        })?;
        rep.confidence_up = up.len();
        rep.confidence_down = down.len();
        rep.open_conflicts = self.conflicts(Some("open"))?.len();
        rep.ms = t0.elapsed().as_millis() as u64;
        let now = vortex_types::now().to_rfc3339();
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO dream_runs(id, report, created_at) VALUES (?1, ?2, ?3)",
                params![EventId::new().to_string(), serde_json::to_string(&rep)?, now],
            )?;
            out.emit(tx, Event::new(EventKind::DreamCompleted, TraceId::new(), serde_json::to_value(&rep)?))?;
            Ok(())
        })?;
        self.bump();
        Ok(rep)
    }

    pub fn dream_runs(&self, limit: usize) -> Result<Vec<serde_json::Value>> {
        Ok(self.log.db().read(|c| {
            let mut st = c.prepare("SELECT report, created_at FROM dream_runs ORDER BY created_at DESC LIMIT ?1")?;
            let v = st
                .query_map([limit as i64], |r| {
                    let rep: String = r.get(0)?;
                    let mut j: serde_json::Value = serde_json::from_str(&rep).unwrap_or_default();
                    j["at"] = serde_json::json!(r.get::<_, String>(1)?);
                    Ok(j)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?)
    }
}
