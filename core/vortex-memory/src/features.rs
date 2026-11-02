//! Featurevectoren voor de neurale layers (W11.05). Vaste volgorde = contract met
//! `vortex-neural::spec` (zelfde namen) en met de datasets.

use crate::policy::PolicyFeatures;
use crate::types::{Memory, MemoryKind, ScoreParts};

fn b(x: bool) -> f32 {
    if x { 1.0 } else { 0.0 }
}

fn kind5(k: MemoryKind) -> [f32; 5] {
    match k {
        MemoryKind::User | MemoryKind::Project => [1.0, 0.0, 0.0, 0.0, 0.0],
        MemoryKind::Semantic | MemoryKind::World => [0.0, 1.0, 0.0, 0.0, 0.0],
        MemoryKind::Procedural | MemoryKind::Summary => [0.0, 0.0, 1.0, 0.0, 0.0],
        MemoryKind::Episodic | MemoryKind::LongTerm => [0.0, 0.0, 0.0, 1.0, 0.0],
        MemoryKind::ShortTerm | MemoryKind::Meta => [0.0, 0.0, 0.0, 0.0, 1.0],
    }
}

/// importance: 8 policy-kenmerken + 5 soortgroepen.
pub fn importance_vector(f: &PolicyFeatures, kind: MemoryKind) -> Vec<f32> {
    let mut v = vec![
        (f.chars as f32 / 1000.0).min(5.0),
        (f.words as f32 / 100.0).min(5.0),
        b(f.explicit_remember),
        b(f.personal),
        b(f.question_only),
        b(f.small_talk),
        b(f.has_numbers),
        b(f.urgent),
    ];
    v.extend(kind5(kind));
    v
}

/// usefulness: scoredelen van één opgehaalde herinnering.
pub fn usefulness_vector(p: &ScoreParts) -> Vec<f32> {
    vec![
        p.vector.unwrap_or(0.0),
        p.bm25_rank.map_or(0.0, |r| 1.0 / (1.0 + r as f32)),
        p.graph_hops.map_or(0.0, |h| 1.0 / (1.0 + h as f32)),
        b(p.temporal),
        (p.rrf * 30.0).min(1.0) as f32,
        p.rerank.unwrap_or(0.0),
        p.recency as f32,
        p.importance as f32,
    ]
}

pub fn lev(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// entity: gelijkenis van twee (genormaliseerde) namen.
pub fn entity_vector(a: &str, b_: &str) -> Vec<f32> {
    let (la, lb) = (a.chars().count().max(1), b_.chars().count().max(1));
    let lev_ratio = 1.0 - lev(a, b_) as f32 / la.max(lb) as f32;
    let ta: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let tb: std::collections::HashSet<&str> = b_.split_whitespace().collect();
    let jac = ta.intersection(&tb).count() as f32 / ta.union(&tb).count().max(1) as f32;
    let prefix = a.chars().zip(b_.chars()).take_while(|(x, y)| x == y).count() as f32 / la.min(lb) as f32;
    let len_ratio = la.min(lb) as f32 / la.max(lb) as f32;
    let same_initial = b(a.chars().next() == b_.chars().next());
    let caps = b(a.chars().next().is_some_and(char::is_uppercase) && b_.chars().next().is_some_and(char::is_uppercase));
    vec![lev_ratio, jac, prefix.min(1.0), len_ratio, same_initial, caps]
}

/// Heuristiek van de entity matcher: bijna-gelijk en niet te kort.
pub fn entity_heuristic(x: &[f32], len: usize) -> f32 {
    if x[0] >= 0.92 && len >= 5 { 1.0 } else { 0.0 }
}

/// decay: leeftijd, gebruik, belang, vertrouwen, inactiviteit, soort.
pub fn decay_vector(m: &Memory, now: chrono::DateTime<chrono::Utc>) -> Vec<f32> {
    let days = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|t| (now - t.with_timezone(&chrono::Utc)).num_hours() as f32 / 24.0)
            .unwrap_or(0.0)
    };
    let k = kind5(m.kind);
    vec![
        days(&m.created_at) / 365.0,
        (1.0 + m.access_count as f32).ln(),
        m.importance as f32,
        m.confidence as f32,
        days(&m.updated_at) / 365.0,
        k[0],
        k[1],
        k[3],
        k[2] + k[4],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_features() {
        let x = entity_vector("rust programming language", "rust programmeertaal");
        assert_eq!(x.len(), 6);
        assert!(x[0] > 0.5 && x[4] == 1.0);
        assert_eq!(entity_vector("abc", "abc")[0], 1.0);
        assert_eq!(entity_heuristic(&entity_vector("syneye", "syneye"), 6), 1.0);
    }
}
