//! Zelfmodel (W12.06): sterktes en zwaktes per taaktype uit eval-resultaten. Metacognitie
//! gebruikt het (zwak taaktype → diep pad + extra zorg); de leeragenda krijgt "weak_task".
// VX:W12.06

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::router::{TaskType, route};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskScore {
    pub score: f64,
    pub n: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelfModel {
    pub by_task: BTreeMap<String, TaskScore>,
}

/// Minimaal aantal taken voor een uitspraak.
pub const MIN_N: usize = 3;
/// Onder deze score is een taaktype "zwak".
pub const WEAK: f64 = 0.6;

fn key(t: TaskType) -> String {
    serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

impl SelfModel {
    /// Uit (prompt, score 0..1)-paren van recente eval-runs; taaktype via de router.
    pub fn from_results(items: &[(String, f64)]) -> Self {
        let mut acc: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for (prompt, score) in items {
            let e = acc.entry(key(route(prompt).task)).or_default();
            e.0 += score;
            e.1 += 1;
        }
        Self { by_task: acc.into_iter().map(|(k, (s, n))| (k, TaskScore { score: s / n as f64, n })).collect() }
    }

    /// Score voor dit taaktype als er genoeg gegevens zijn.
    pub fn score(&self, t: TaskType) -> Option<f64> {
        self.by_task.get(&key(t)).filter(|s| s.n >= MIN_N).map(|s| s.score)
    }

    pub fn weak(&self) -> Vec<(String, f64)> {
        self.by_task.iter().filter(|(_, s)| s.n >= MIN_N && s.score < WEAK).map(|(k, s)| (k.clone(), s.score)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_task_types() {
        let items: Vec<(String, f64)> = vec![
            ("Bereken 17 × 23 + 4".into(), 0.0),
            ("Bereken 3 + 4 × 2".into(), 0.0),
            ("Bereken 12 × 12".into(), 1.0),
            ("hoi!".into(), 1.0),
        ];
        let m = SelfModel::from_results(&items);
        assert_eq!(m.score(TaskType::Math).map(|s| (s * 100.0).round()), Some(33.0));
        assert_eq!(m.score(TaskType::Chat), None, "te weinig gegevens");
        assert_eq!(m.weak().len(), 1);
    }
}
