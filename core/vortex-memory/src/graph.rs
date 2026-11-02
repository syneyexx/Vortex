//! Temporal knowledge graph (W12.04) + graph-redeneren (W12.05).
//! Feiten (edges) hebben geldig-van/tot; "wat gold op datum X" en de historie van een relatie
//! zijn op te vragen. Regels (zonder LM): transitief, symmetrisch, invers en ketens; afgeleide
//! feiten krijgen `derived = 1`, de regel en hun steun (`edge_support`) en worden bij elke
//! redeneer-ronde opnieuw berekend (weg = weg). Consistentie: functionele relaties (bv. "woont
//! in") met twee waarden tegelijk → conflict voor het conflict-team.
// VX:W12.04

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventId, EventKind, TraceId};

use crate::error::Result;
use crate::store::MemoryStore;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub src: String,
    pub rel: String,
    pub dst: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub derived: bool,
    pub rule: Option<String>,
    pub memory_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// a r b, b r c ⇒ a r c
    Transitive,
    /// a r b ⇒ b r a
    Symmetric,
    /// a r b ⇒ b then a
    Inverse,
    /// a r b, b r2 c ⇒ a then c
    Chain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub kind: RuleKind,
    pub rel: String,
    #[serde(default)]
    pub rel2: Option<String>,
    #[serde(default)]
    pub then: Option<String>,
}

impl Rule {
    fn new(name: &str, kind: RuleKind, rel: &str, rel2: Option<&str>, then: Option<&str>) -> Self {
        Self { name: name.into(), kind, rel: rel.into(), rel2: rel2.map(String::from), then: then.map(String::from) }
    }
}

/// Standaardregels (NL + EN). Uit te breiden via `config/rules.toml`.
pub fn default_rules() -> Vec<Rule> {
    use RuleKind::*;
    let mut v = Vec::new();
    for r in
        ["is een", "is_a", "deel van", "onderdeel van", "part_of", "ligt in", "located_in", "valt onder", "subclass_of"]
    {
        v.push(Rule::new(&format!("transitief: {r}"), Transitive, r, None, None));
    }
    for r in
        ["getrouwd met", "married_to", "broer van", "zus van", "sibling_of", "partner van", "collega van", "grenst aan"]
    {
        v.push(Rule::new(&format!("symmetrisch: {r}"), Symmetric, r, None, None));
    }
    v.push(Rule::new("invers: ouder/kind", Inverse, "ouder van", None, Some("kind van")));
    v.push(Rule::new("invers: kind/ouder", Inverse, "kind van", None, Some("ouder van")));
    v.push(Rule::new("invers: parent/child", Inverse, "parent_of", None, Some("child_of")));
    v.push(Rule::new("invers: bevat", Inverse, "bevat", None, Some("deel van")));
    v.push(Rule::new("keten: werkt bij + ligt in", Chain, "werkt bij", Some("ligt in"), Some("werkt in")));
    v.push(Rule::new("keten: woont in + ligt in", Chain, "woont in", Some("ligt in"), Some("woont in regio")));
    v.push(Rule::new("keten: works_at + located_in", Chain, "works_at", Some("located_in"), Some("works_in")));
    v
}

/// Relaties met hooguit één geldige waarde tegelijk.
pub const FUNCTIONAL: &[&str] = &[
    "woont in",
    "lives_in",
    "geboren in",
    "born_in",
    "geboortedatum",
    "hoofdstad van",
    "capital_of",
    "ceo van",
    "getrouwd met",
];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReasonReport {
    pub derived: usize,
    pub iterations: usize,
    pub conflicts_found: usize,
    pub by_rule: HashMap<String, usize>,
}

/// Overlappen twee geldigheidsperiodes? (ISO-strings vergelijken lexicografisch.)
fn overlaps(a: (&Option<String>, &Option<String>), b: (&Option<String>, &Option<String>)) -> bool {
    let start = |x: &Option<String>| x.clone().unwrap_or_default();
    let end = |x: &Option<String>| x.clone().unwrap_or_else(|| "9999".into());
    start(a.0) < end(b.1) && start(b.0) < end(a.1)
}

fn row_fact(r: &rusqlite::Row<'_>) -> rusqlite::Result<Fact> {
    Ok(Fact {
        id: r.get(0)?,
        src: r.get(1)?,
        rel: r.get(2)?,
        dst: r.get(3)?,
        valid_from: r.get(4)?,
        valid_to: r.get(5)?,
        confidence: r.get(6)?,
        derived: r.get::<_, i64>(7)? == 1,
        rule: r.get(8)?,
        memory_id: r.get(9)?,
    })
}

/// Effectieve geldigheid = die van de edge, anders die van zijn herinnering (conflictbeslissingen
/// en rollbacks werken zo vanzelf door in de graaf).
const FACT_SQL: &str = "SELECT e.id, s.name, e.rel, d.name,
        COALESCE(e.valid_from, m.valid_from, m.event_time) AS vf, COALESCE(e.valid_to, m.valid_to) AS vt,
        e.confidence * COALESCE(m.confidence, 1.0), e.derived, e.rule, e.memory_id
     FROM edges e JOIN entities s ON s.id = e.src JOIN entities d ON d.id = e.dst
     LEFT JOIN memories m ON m.id = e.memory_id";

impl MemoryStore {
    fn entity_id(&self, name: &str) -> Result<Option<String>> {
        let norm = name.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(self.log.db().read(|c| {
            Ok(c.query_row("SELECT id FROM entities WHERE norm = ?1", [&norm], |r| r.get(0)).optional()?)
        })?)
    }

    /// Feiten over een entiteit die golden op `at` (ISO-datum; None = nu). Zonder `related_to`-ruis.
    pub fn facts_about(&self, entity: &str, at: Option<&str>) -> Result<Vec<Fact>> {
        let Some(eid) = self.entity_id(entity)? else { return Ok(vec![]) };
        let at = at.map(String::from).unwrap_or_else(|| vortex_types::now().to_rfc3339());
        Ok(self.log.db().read(|c| {
            let sql = format!(
                "{FACT_SQL} WHERE (e.src = ?1 OR e.dst = ?1) AND e.rel != 'related_to'
                 AND (m.id IS NULL OR m.stale = 0)
                 AND (vf IS NULL OR vf <= ?2) AND (vt IS NULL OR vt > ?2)
                 ORDER BY e.derived, e.confidence DESC"
            );
            let mut st = c.prepare(&sql)?;
            let v = st.query_map(params![eid, at], row_fact)?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?)
    }

    /// Volledige historie (ook verlopen feiten) van een entiteit, optioneel één relatie.
    pub fn fact_history(&self, entity: &str, rel: Option<&str>) -> Result<Vec<Fact>> {
        let Some(eid) = self.entity_id(entity)? else { return Ok(vec![]) };
        Ok(self.log.db().read(|c| {
            let sql = format!("{FACT_SQL} WHERE e.src = ?1 AND e.rel != 'related_to' AND (?2 IS NULL OR e.rel = ?2) ORDER BY COALESCE(vf, e.created_at)");
            let mut st = c.prepare(&sql)?;
            let v = st.query_map(params![eid, rel], row_fact)?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?)
    }

    /// Graph-redeneren: afgeleide feiten opnieuw berekenen (fixpunt, max 4 rondes) +
    /// consistentiecontrole op functionele relaties.
    pub fn reason(&self, rules: &[Rule]) -> Result<ReasonReport> {
        let mut rep = ReasonReport::default();
        // Basisfeiten (niet afgeleid, met herinnering die niet verouderd is).
        type E = (String, String, String, String, Option<String>, Option<String>, f64);
        let base: Vec<E> = self.log.db().read(|c| {
            let mut st = c.prepare(
                "SELECT e.id, e.src, e.rel, e.dst, COALESCE(e.valid_from, m.valid_from, m.event_time), COALESCE(e.valid_to, m.valid_to),
                        e.confidence * COALESCE(m.confidence, 1.0) FROM edges e
                 LEFT JOIN memories m ON m.id = e.memory_id
                 WHERE e.derived = 0 AND e.rel != 'related_to' AND (m.id IS NULL OR m.stale = 0)",
            )?;
            let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })?;
        let mut known: HashSet<(String, String, String)> =
            base.iter().map(|e| (e.1.clone(), e.2.clone(), e.3.clone())).collect();
        let mut all: Vec<E> = base.clone();
        let mut new_facts: Vec<(E, String, Vec<String>)> = Vec::new();
        for it in 0..4 {
            rep.iterations = it + 1;
            let mut added: Vec<(E, String, Vec<String>)> = Vec::new();
            let by_src: HashMap<&str, Vec<&E>> = all.iter().fold(HashMap::new(), |mut m, e| {
                m.entry(e.1.as_str()).or_default().push(e);
                m
            });
            let mk = |a: &str, rel: &str, b: &str, sup: &[&E], rule: &str| -> (E, String, Vec<String>) {
                let vf = sup.iter().filter_map(|e| e.4.clone()).max();
                let vt = sup.iter().filter_map(|e| e.5.clone()).min();
                let conf = sup.iter().map(|e| e.6).fold(1.0, f64::min) * 0.9;
                (
                    (EventId::new().to_string(), a.into(), rel.into(), b.into(), vf, vt, conf),
                    rule.into(),
                    sup.iter().map(|e| e.0.clone()).collect(),
                )
            };
            for r in rules {
                for e in all.iter().filter(|e| e.2 == r.rel) {
                    match r.kind {
                        RuleKind::Symmetric => added.push(mk(&e.3, &r.rel, &e.1, &[e], &r.name)),
                        RuleKind::Inverse => {
                            if let Some(t) = &r.then {
                                added.push(mk(&e.3, t, &e.1, &[e], &r.name));
                            }
                        }
                        RuleKind::Transitive | RuleKind::Chain => {
                            let rel2 = if r.kind == RuleKind::Transitive {
                                r.rel.as_str()
                            } else {
                                r.rel2.as_deref().unwrap_or("")
                            };
                            let then = if r.kind == RuleKind::Transitive {
                                r.rel.as_str()
                            } else {
                                r.then.as_deref().unwrap_or("")
                            };
                            for f in by_src.get(e.3.as_str()).into_iter().flatten().filter(|f| f.2 == rel2) {
                                if f.3 != e.1 && overlaps((&e.4, &e.5), (&f.4, &f.5)) {
                                    added.push(mk(&e.1, then, &f.3, &[e, f], &r.name));
                                }
                            }
                        }
                    }
                }
            }
            let mut fresh = 0;
            for (e, rule, sup) in added {
                if e.1 != e.3 && known.insert((e.1.clone(), e.2.clone(), e.3.clone())) {
                    all.push(e.clone());
                    new_facts.push((e, rule, sup));
                    fresh += 1;
                }
            }
            if fresh == 0 {
                break;
            }
        }
        // Afgeleide feiten vervangen (één transactie).
        let now = vortex_types::now().to_rfc3339();
        self.log.transact(|tx, out| {
            tx.execute("DELETE FROM edge_support WHERE derived_edge IN (SELECT id FROM edges WHERE derived = 1)", [])?;
            tx.execute("DELETE FROM edges WHERE derived = 1", [])?;
            for (e, rule, sup) in &new_facts {
                tx.execute(
                    "INSERT INTO edges(id, src, rel, dst, memory_id, valid_from, valid_to, confidence, created_at, derived, rule) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, 1, ?9)",
                    params![e.0, e.1, e.2, e.3, e.4, e.5, e.6, now, rule],
                )?;
                for s in sup {
                    tx.execute("INSERT OR IGNORE INTO edge_support(derived_edge, from_edge) VALUES (?1, ?2)", params![e.0, s])?;
                }
            }
            out.emit(tx, Event::new(EventKind::GraphReasoned, TraceId::new(), serde_json::json!({"derived": new_facts.len()})))?;
            Ok(())
        })?;
        rep.derived = new_facts.len();
        for (_, r, _) in &new_facts {
            *rep.by_rule.entry(r.clone()).or_default() += 1;
        }
        // Consistentie: functionele relatie met twee waarden in overlappende periodes → conflict.
        let mut seen = HashSet::new();
        for a in base.iter().filter(|e| FUNCTIONAL.contains(&e.2.as_str())) {
            for b in base.iter().filter(|b| b.1 == a.1 && b.2 == a.2 && b.3 != a.3 && b.0 > a.0) {
                if !overlaps((&a.4, &a.5), (&b.4, &b.5)) {
                    continue;
                }
                let mems: Option<(String, String)> = self.log.db().read(|c| {
                    let m = |id: &str| {
                        c.query_row("SELECT memory_id FROM edges WHERE id = ?1", [id], |r| {
                            r.get::<_, Option<String>>(0)
                        })
                        .optional()
                    };
                    Ok(m(&a.0)?.flatten().zip(m(&b.0)?.flatten()))
                })?;
                let Some((ma, mb)) = mems else { continue };
                if ma == mb || !seen.insert((ma.clone(), mb.clone())) {
                    continue;
                }
                let exists: bool = self.log.db().read(|c| {
                    Ok(c.query_row(
                        "SELECT 1 FROM conflicts WHERE (a_memory = ?1 AND b_memory = ?2) OR (a_memory = ?2 AND b_memory = ?1)",
                        params![ma, mb],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some())
                })?;
                if !exists {
                    self.log.transact(|tx, out| {
                        let id = format!("cnf_{}", EventId::new().to_string().trim_start_matches("evt_"));
                        tx.execute(
                            "INSERT INTO conflicts(id, a_memory, b_memory, reason, status, created_at) VALUES (?1, ?2, ?3, ?4, 'open', ?5)",
                            params![id, ma, mb, format!("functionele relatie '{}' heeft twee waarden tegelijk", a.2), now],
                        )?;
                        out.emit(tx, Event::new(EventKind::ConflictDetected, TraceId::new(), serde_json::json!({"conflict": id, "reason": "graph-consistentie"})))?;
                        Ok(())
                    })?;
                    rep.conflicts_found += 1;
                }
            }
        }
        Ok(rep)
    }
}
