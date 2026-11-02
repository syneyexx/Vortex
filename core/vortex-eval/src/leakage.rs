//! Lekkagecontrole (W06.04): eval-inhoud mag niet via geheugen, caches, trainingsdata of
//! near-duplicates beschikbaar zijn. Exacte hash + woord-5-gram-shingles (Jaccard).
// VX:W06.04

use std::collections::HashSet;

use serde::Serialize;
use vortex_core::config::fnv1a64;

fn norm(t: &str) -> Vec<String> {
    t.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

pub fn shingles(t: &str, n: usize) -> HashSet<u64> {
    let w = norm(t);
    if w.len() < n {
        return std::iter::once(fnv1a64(w.join(" ").as_bytes())).collect();
    }
    w.windows(n).map(|s| fnv1a64(s.join(" ").as_bytes())).collect()
}

pub fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let i = a.intersection(b).count() as f64;
    i / (a.len() as f64 + b.len() as f64 - i)
}

/// Containment: welk deel van de eval-tekst komt voor in de kandidaat (robuuster dan Jaccard
/// als de kandidaat veel langer is).
pub fn containment(eval: &HashSet<u64>, cand: &HashSet<u64>) -> f64 {
    if eval.is_empty() {
        return 0.0;
    }
    eval.intersection(cand).count() as f64 / eval.len() as f64
}

#[derive(Debug, Clone, Serialize)]
pub struct Leak {
    pub eval_id: String,
    pub where_: String,
    pub overlap: f64,
}

/// Vergelijk eval-teksten met een corpus (geheugen, trainingsdata, …).
pub fn check(evals: &[(String, String)], corpus: &[(String, String)], threshold: f64) -> Vec<Leak> {
    let cs: Vec<(String, HashSet<u64>)> = corpus.iter().map(|(id, t)| (id.clone(), shingles(t, 5))).collect();
    let mut out = Vec::new();
    for (eid, et) in evals {
        let es = shingles(et, 5);
        for (cid, c) in &cs {
            let o = containment(&es, c);
            if o >= threshold {
                out.push(Leak { eval_id: eid.clone(), where_: cid.clone(), overlap: o });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_near_duplicates_not_unrelated() {
        let evals =
            vec![("t1".to_string(), "Hoeveel cores heeft een Ryzen 9 5950X processor eigenlijk precies?".to_string())];
        let corpus = vec![
            (
                "mem_a".to_string(),
                "Vraag: hoeveel cores heeft een Ryzen 9 5950X processor eigenlijk precies? Antwoord: 16.".to_string(),
            ),
            ("mem_b".to_string(), "De kat zit op de mat en kijkt naar buiten.".to_string()),
        ];
        let l = check(&evals, &corpus, 0.6);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].where_, "mem_a");
        let s = shingles("a b c d e f", 5);
        assert!((jaccard(&s, &s) - 1.0).abs() < 1e-9);
        assert_eq!(jaccard(&s, &shingles("x y z w v u", 5)), 0.0);
    }
}
