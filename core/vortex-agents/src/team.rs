//! Dynamische teamsamenstelling door metacognitie (W09.02): eenvoudig → geen team (alleen
//! Synthesizer = het gewone antwoord); moeilijk → Planner/Solvers/Fact-checker/Critic/
//! Specialist/Judge, met rondes en budget naar moeilijkheid en modus.

use serde::{Deserialize, Serialize};
use vortex_cognition::{Path, Route, Strategy, TaskType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamPlan {
    pub planner: bool,
    pub solvers: usize,
    pub fact_checker: bool,
    pub critic: bool,
    /// "code" | "math" | none
    pub specialist: Option<String>,
    pub judge: bool,
    pub max_rounds: usize,
    /// Solvers tegelijk (llama-server met meerdere slots) of na elkaar.
    pub parallel: bool,
    pub why: Vec<String>,
}

impl TeamPlan {
    pub fn roles(&self) -> Vec<String> {
        let mut r = Vec::new();
        if self.planner {
            r.push("Planner".to_string());
        }
        r.push(format!("Solvers ×{}", self.solvers));
        if self.fact_checker {
            r.push("Fact-checker".into());
        }
        if self.critic {
            r.push("Critic".into());
        }
        if let Some(s) = &self.specialist {
            r.push(format!("Specialist ({s})"));
        }
        if self.judge {
            r.push("Judge".into());
        }
        r.push("Synthesizer".into());
        r
    }
}

/// Team kiezen. `None` = geen team nodig.
pub fn compose(route: &Route, strategy: &Strategy, mode: &str) -> Option<TeamPlan> {
    if strategy.path == Path::Fast {
        return None;
    }
    let deep = mode == "deep" || mode == "research";
    let hard = route.difficulty >= 0.45;
    if !deep && !hard {
        return None;
    }
    let mut why = vec![format!("moeilijkheid {:.2}", route.difficulty)];
    if deep {
        why.push(format!("modus {mode}"));
    }
    let solvers = if route.difficulty >= 0.65 || deep { 3 } else { 2 };
    let planner =
        matches!(route.task, TaskType::Planning | TaskType::Research | TaskType::Analysis) || route.features.words > 40;
    let fact_checker =
        matches!(route.task, TaskType::Factual | TaskType::Research | TaskType::Analysis | TaskType::Personal);
    let specialist = match route.task {
        TaskType::Code => Some("code".to_string()),
        TaskType::Math => Some("math".to_string()),
        _ => None,
    };
    if let Some(s) = &specialist {
        why.push(format!("objectieve check: {s}"));
    }
    Some(TeamPlan {
        planner,
        solvers,
        fact_checker,
        critic: true,
        specialist,
        judge: true,
        max_rounds: if deep { 3 } else { 2 },
        parallel: true,
        why,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vortex_cognition::{Signals, decide, route};

    fn plan(q: &str, mode: &str) -> Option<TeamPlan> {
        let r = route(q);
        let s = decide(&Signals {
            route: r.clone(),
            mode: mode.into(),
            best_memory: Some(0.8),
            conflicts: 0,
            tools: true,
            research: false,
            self_score: None,
        });
        compose(&r, &s, mode)
    }

    #[test]
    fn simple_gets_no_team_hard_gets_full_team() {
        assert!(plan("hoi!", "adaptive").is_none());
        assert!(plan("hoe laat is het?", "quick").is_none());
        let t = plan("Vergelijk de voor- en nadelen van SQLite en Postgres voor een lokale AI-assistent en analyseer welke beter past bij veel kleine schrijfacties", "adaptive").unwrap();
        assert!(t.planner && t.critic && t.judge && t.fact_checker);
        let c = plan("Waarom faalt deze Rust-code met error[E0599]? fn main() { let v = vec![1]; v.foo(); }", "deep")
            .unwrap();
        assert_eq!(c.specialist.as_deref(), Some("code"));
        assert_eq!(c.solvers, 3);
        assert_eq!(c.max_rounds, 3);
    }
}
