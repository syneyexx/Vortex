//! Positionele contexttests per model (W17.07): een "naald" (uniek feit) op verschillende
//! posities (begin … eind) in contexten van verschillende lengte; vraag aan het eind. Laat zien
//! waar een model informatie mist ("lost in the middle") → stuurt de contextassemblage.
// VX:W17.07

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{Check, EvalTask, Suite};
use crate::runner::TaskResult;

pub const POSITIONS: &[f64] = &[0.0, 0.25, 0.5, 0.75, 1.0];

const FILLER: &[&str] = &[
    "De gemeenteraad besprak de begroting voor het komende jaar en de plannen voor fietspaden.",
    "In het voorjaar bloeien de tulpen langs de dijken en trekken veel bezoekers naar de bollenstreek.",
    "Een goed onderhouden fiets gaat jaren mee als je de ketting regelmatig smeert.",
    "Het weerbericht voorspelt wisselvallig weer met af en toe een bui en matige wind uit het westen.",
    "De bibliotheek organiseert elke donderdag een leesclub voor volwassenen en jongeren.",
    "Tijdens de renovatie werden de oude kozijnen vervangen door dubbel glas.",
];

/// Ongeveer `tokens` tokens tekst (~1,3 token per woord) met de naald op relatieve positie `pos`.
pub fn haystack(tokens: usize, pos: f64, needle: &str) -> String {
    let words_needed = (tokens as f64 / 1.3) as usize;
    let mut sents = Vec::new();
    let mut n = 0;
    let mut i = 0;
    while n < words_needed {
        let s = FILLER[i % FILLER.len()];
        n += s.split_whitespace().count();
        sents.push(format!("{} ({})", s, i + 1));
        i += 1;
    }
    let at = ((sents.len() as f64) * pos.clamp(0.0, 1.0)).round() as usize;
    sents.insert(at.min(sents.len()), needle.to_string());
    sents.join(" ")
}

/// Suite: per lengte × positie één taak met een eigen naald.
pub fn suite(lengths: &[usize]) -> Suite {
    let mut tasks = Vec::new();
    for (li, len) in lengths.iter().enumerate() {
        for (pi, pos) in POSITIONS.iter().enumerate() {
            let code = 1000 + li * 97 + pi * 13;
            let needle = format!("Belangrijk: de toegangscode voor archiefkast {} is {code}.", li * 10 + pi);
            let prompt = format!(
                "Lees de tekst en beantwoord daarna de vraag.\n\n{}\n\nVraag: wat is de toegangscode voor archiefkast {}? Antwoord alleen met de code.",
                haystack(*len, *pos, &needle),
                li * 10 + pi
            );
            tasks.push(EvalTask {
                id: format!("pos_{len}_{:03}", (pos * 100.0) as u32),
                prompt,
                checks: vec![Check::Contains { all: vec![code.to_string()] }],
                memory: vec![],
                requires: None,
                mode: Some("quick".into()),
                tags: vec!["positional".into()],
                difficulty: Some(1),
                counterfactual: vec![],
            });
        }
    }
    Suite { name: "positional".into(), tasks }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionalReport {
    /// lengte → (positie in % → geslaagd 0/1)
    pub grid: BTreeMap<usize, BTreeMap<u32, f64>>,
    pub by_position: BTreeMap<u32, f64>,
    /// Zwakste positie (laagste score) — "lost in the middle" als die in het midden zit.
    pub weakest: Option<u32>,
}

pub fn report(results: &[TaskResult]) -> PositionalReport {
    let mut grid: BTreeMap<usize, BTreeMap<u32, f64>> = BTreeMap::new();
    let mut by: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
    for r in results {
        let mut it = r.task_id.trim_start_matches("pos_").split('_');
        let (Some(l), Some(p)) =
            (it.next().and_then(|x| x.parse().ok()), it.next().and_then(|x| x.parse::<u32>().ok()))
        else {
            continue;
        };
        grid.entry(l).or_default().insert(p, r.score);
        let e = by.entry(p).or_default();
        e.0 += r.score;
        e.1 += 1;
    }
    let by_position: BTreeMap<u32, f64> = by.into_iter().map(|(k, (s, n))| (k, s / n as f64)).collect();
    let weakest = by_position.iter().min_by(|a, b| a.1.total_cmp(b.1)).map(|x| *x.0);
    PositionalReport { grid, by_position, weakest }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needle_lands_at_position() {
        let h = haystack(2000, 0.5, "NAALD");
        let i = h.find("NAALD").unwrap_or(0) as f64 / h.len() as f64;
        assert!((0.4..0.6).contains(&i), "{i}");
        assert!(haystack(2000, 0.0, "NAALD").starts_with("NAALD"));
        let s = suite(&[1000, 4000]);
        assert_eq!(s.tasks.len(), 10);
        let res: Vec<TaskResult> = s
            .tasks
            .iter()
            .map(|t| TaskResult {
                task_id: t.id.clone(),
                status: "pass".into(),
                score: if t.id.ends_with("_050") { 0.0 } else { 1.0 },
                latency_ms: 1,
                output: String::new(),
                detail: String::new(),
            })
            .collect();
        let r = report(&res);
        assert_eq!(r.weakest, Some(50), "lost in the middle zichtbaar");
    }
}
