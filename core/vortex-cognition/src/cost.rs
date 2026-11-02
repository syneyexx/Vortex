//! Kostengebaseerde planselectie (W17.12): kies het pad met de beste verwachte waarde per
//! seconde — kosten uit eigen metingen (gemiddelde runduur per pad en taaktype), winst uit het
//! zelfmodel (eval-score per taaktype). Diep denken loont alleen waar het iets oplevert.
// VX:W17.12

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::metacog::Path;
use crate::router::TaskType;

/// Gemeten gemiddelde duur (ms) per (pad, taaktype).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanCosts {
    pub ms: HashMap<String, f64>,
    pub n: HashMap<String, usize>,
}

impl PlanCosts {
    pub fn key(p: Path, t: TaskType) -> String {
        format!("{p:?}/{t:?}")
    }
    pub fn get(&self, p: Path, t: TaskType) -> Option<f64> {
        let k = Self::key(p, t);
        self.ms.get(&k).copied().filter(|_| self.n.get(&k).copied().unwrap_or(0) >= 3)
    }
}

/// Fast i.p.v. Deliberate als (a) het zelfmodel dit taaktype al goed doet (≥ 0,85) en (b) diep
/// denken gemeten veel duurder is (> 4× én > budget). Geeft de reden terug.
pub fn downgrade(costs: &PlanCosts, task: TaskType, self_score: Option<f64>, budget_ms: f64) -> Option<String> {
    let (fast, deep) = (costs.get(Path::Fast, task)?, costs.get(Path::Deliberate, task)?);
    let good = self_score.is_some_and(|s| s >= 0.85);
    (good && deep > fast * 4.0 && deep > budget_ms).then(|| {
        format!(
            "kosten: diep {:.0} s vs snel {:.0} s voor {task:?} en het zelfmodel scoort al {:.0}% → snel",
            deep / 1000.0,
            fast / 1000.0,
            self_score.unwrap_or(0.0) * 100.0
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downgrade_only_when_cheap_wins() {
        let mut c = PlanCosts::default();
        for (p, ms) in [(Path::Fast, 2000.0), (Path::Deliberate, 30_000.0)] {
            c.ms.insert(PlanCosts::key(p, TaskType::Factual), ms);
            c.n.insert(PlanCosts::key(p, TaskType::Factual), 5);
        }
        assert!(downgrade(&c, TaskType::Factual, Some(0.9), 10_000.0).is_some());
        assert!(downgrade(&c, TaskType::Factual, Some(0.6), 10_000.0).is_none(), "zwak taaktype → diep blijft");
        assert!(downgrade(&c, TaskType::Code, Some(0.9), 10_000.0).is_none(), "geen metingen → niets");
    }
}
