//! Metamorfische + counterfactual tests (W17.04). Metamorf: betekenis-behoudende wijzigingen
//! (ruis, opmaak, beleefdheid, irrelevante context) mogen de uitkomst niet veranderen.
//! Counterfactual: één wezenlijke wijziging (in de set vastgelegd) moet het antwoord wél
//! veranderen zoals verwacht. Uitkomst: consistentie + counterfactual-slagingspercentage.
// VX:W17.04

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{EvalTask, Suite};
use crate::runner::TaskResult;

pub const RELATIONS: &[&str] = &["ruis", "opmaak", "beleefd", "context"];

fn transform(rel: &str, p: &str) -> String {
    match rel {
        "ruis" => format!("{p}\n\n(Terzijde: het regent vandaag in Utrecht; dat staat hier los van.)"),
        "opmaak" => format!("\n\n   {}   \n", p.replace(". ", ".\n").replace("? ", "?\n")),
        "beleefd" => format!("Hoi! Zou je me willen helpen met het volgende? {p} Alvast bedankt."),
        "context" => format!("Ik ben bezig met een project over vogels en heb net koffie gezet.\n{p}"),
        _ => p.to_string(),
    }
}

/// Afgeleide suite: per taak de metamorfe varianten (`id~relatie`) en counterfactuals (`id~cfN`).
pub fn derive(s: &Suite) -> Suite {
    let mut tasks = Vec::new();
    for t in s.tasks.iter().filter(|t| t.memory.is_empty()) {
        for rel in RELATIONS {
            tasks.push(EvalTask {
                id: format!("{}~{rel}", t.id),
                prompt: transform(rel, &t.prompt),
                counterfactual: vec![],
                ..t.clone()
            });
        }
        for (i, cf) in t.counterfactual.iter().enumerate() {
            tasks.push(EvalTask {
                id: format!("{}~cf{i}", t.id),
                prompt: cf.prompt.clone(),
                checks: cf.checks.clone(),
                counterfactual: vec![],
                ..t.clone()
            });
        }
    }
    Suite { name: format!("{}~metamorf", s.name), tasks }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaReport {
    pub pairs: usize,
    /// Aandeel varianten met dezelfde uitkomst (geslaagd/niet) als het origineel.
    pub consistency: f64,
    pub by_relation: HashMap<String, f64>,
    pub counterfactuals: usize,
    pub counterfactual_pass: f64,
    /// Taken die omvallen door een betekenis-behoudende wijziging.
    pub fragile: Vec<String>,
}

pub fn report(original: &[TaskResult], derived: &[TaskResult]) -> MetaReport {
    let pass = |r: &TaskResult| r.status == "pass" || r.score >= 0.999;
    let orig: HashMap<&str, bool> =
        original.iter().filter(|r| r.status != "skip").map(|r| (r.task_id.as_str(), pass(r))).collect();
    let (mut same, mut total) = (0usize, 0usize);
    let mut by: HashMap<String, (usize, usize)> = HashMap::new();
    let (mut cf_n, mut cf_ok) = (0usize, 0usize);
    let mut fragile = Vec::new();
    for r in derived.iter().filter(|r| r.status != "skip") {
        let Some((base, rel)) = r.task_id.split_once('~') else { continue };
        if rel.starts_with("cf") {
            cf_n += 1;
            cf_ok += usize::from(pass(r));
            continue;
        }
        let Some(o) = orig.get(base) else { continue };
        total += 1;
        let e = by.entry(rel.to_string()).or_default();
        e.1 += 1;
        if *o == pass(r) {
            same += 1;
            e.0 += 1;
        } else {
            fragile.push(r.task_id.clone());
        }
    }
    MetaReport {
        pairs: total,
        consistency: if total > 0 { same as f64 / total as f64 } else { 1.0 },
        by_relation: by.into_iter().map(|(k, (a, b))| (k, a as f64 / b.max(1) as f64)).collect(),
        counterfactuals: cf_n,
        counterfactual_pass: if cf_n > 0 { cf_ok as f64 / cf_n as f64 } else { 1.0 },
        fragile,
    }
}
