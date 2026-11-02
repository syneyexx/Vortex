//! Meta-memory (W12.02): wat Vortex weet, hoe zeker, en waar de gaten zitten (→ leeragenda).
// VX:W12.02

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::Result;
use crate::store::MemoryStore;

#[derive(Debug, Clone, Default, Serialize)]
pub struct KindStat {
    pub count: i64,
    pub avg_confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EntityStat {
    pub name: String,
    pub facts: i64,
    pub mentions: i64,
    pub avg_confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetaReport {
    pub total: i64,
    pub by_kind: BTreeMap<String, KindStat>,
    pub stale: i64,
    pub cold: i64,
    pub open_conflicts: i64,
    pub disputed: i64,
    pub derived_facts: i64,
    /// Waar Vortex het meest over weet.
    pub strongest: Vec<EntityStat>,
    /// Kennisgaten: vaak genoemd in gesprekken, weinig feiten.
    pub gaps: Vec<EntityStat>,
}

impl MemoryStore {
    pub fn meta_report(&self) -> Result<MetaReport> {
        Ok(self.log.db().read(|c| {
            let mut r = MetaReport::default();
            let mut st = c.prepare("SELECT kind, COUNT(*), AVG(confidence) FROM memories WHERE stale = 0 GROUP BY kind")?;
            for row in st.query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?, x.get::<_, f64>(2)?)))? {
                let (k, n, a) = row?;
                r.total += n;
                r.by_kind.insert(k, KindStat { count: n, avg_confidence: a });
            }
            r.stale = c.query_row("SELECT COUNT(*) FROM memories WHERE stale = 1", [], |x| x.get(0))?;
            r.cold = c.query_row("SELECT COUNT(*) FROM memories WHERE tier = 'cold'", [], |x| x.get(0))?;
            r.open_conflicts = c.query_row("SELECT COUNT(*) FROM conflicts WHERE status = 'open'", [], |x| x.get(0))?;
            r.disputed = c.query_row("SELECT COUNT(*) FROM conflicts WHERE status = 'disputed'", [], |x| x.get(0))?;
            r.derived_facts = c.query_row("SELECT COUNT(*) FROM edges WHERE derived = 1", [], |x| x.get(0))?;
            // Per entiteit: feiten (randen, geen related_to) en vermeldingen in gesprekken.
            let mut st = c.prepare(
                "SELECT e.name,
                    (SELECT COUNT(*) FROM edges g WHERE (g.src = e.id OR g.dst = e.id) AND g.rel != 'related_to') AS facts,
                    (SELECT COUNT(*) FROM memory_entities me JOIN memories m ON m.id = me.memory_id WHERE me.entity_id = e.id AND m.kind = 'episodic') AS mentions,
                    COALESCE((SELECT AVG(m.confidence) FROM memory_entities me JOIN memories m ON m.id = me.memory_id WHERE me.entity_id = e.id AND m.stale = 0), 0)
                 FROM entities e",
            )?;
            let mut all: Vec<EntityStat> = st
                .query_map([], |x| Ok(EntityStat { name: x.get(0)?, facts: x.get(1)?, mentions: x.get(2)?, avg_confidence: x.get(3)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            all.sort_by(|a, b| b.facts.cmp(&a.facts).then(b.avg_confidence.total_cmp(&a.avg_confidence)));
            r.strongest = all.iter().filter(|e| e.facts > 0).take(15).cloned().collect();
            let mut gaps: Vec<EntityStat> = all.into_iter().filter(|e| e.mentions >= 2 && e.facts < 2).collect();
            gaps.sort_by_key(|g| std::cmp::Reverse(g.mentions));
            gaps.truncate(15);
            r.gaps = gaps;
            Ok(r)
        })?)
    }
}
