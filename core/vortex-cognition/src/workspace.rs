//! Global Workspace (W07.02) + doelstapel (W07.03): het gedeelde "schoolbord" per taak. Alle
//! modules lezen/schrijven hier; elke wijziging is een event (one brain, replay).
// VX:W07.02

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, RunId, TraceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Open,
    Active,
    Done,
    Dropped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: usize,
    pub text: String,
    pub parent: Option<usize>,
    pub depends_on: Vec<usize>,
    pub status: GoalStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub text: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub goal: String,
    pub goals: Vec<Goal>,
    pub memories: Vec<String>,
    pub hypotheses: Vec<Hypothesis>,
    pub open_questions: Vec<String>,
    pub uncertainties: Vec<String>,
    pub notes: Vec<String>,
    pub steering: Vec<String>,
    pub strategy: Option<serde_json::Value>,
    pub version: u64,
}

/// Workspace van één run.
#[derive(Debug, Clone)]
pub struct Workspace {
    run: RunId,
    trace: TraceId,
    board: Arc<RwLock<Board>>,
    log: Option<EventLog>,
}

impl Workspace {
    pub fn new(run: RunId, trace: TraceId, goal: &str, log: Option<EventLog>) -> Self {
        let b = Board { goal: goal.into(), ..Default::default() };
        let ws = Self { run, trace, board: Arc::new(RwLock::new(b)), log };
        ws.emit("goal", serde_json::json!({"goal": goal}));
        ws
    }

    fn emit(&self, what: &str, detail: serde_json::Value) {
        if let Some(l) = &self.log {
            let v = self.board.read().version;
            let _ = l.append(
                Event::new(
                    EventKind::WorkspaceUpdated,
                    self.trace,
                    serde_json::json!({"what": what, "version": v, "detail": detail}),
                )
                .for_run(self.run),
            );
        }
    }

    fn write<T>(&self, what: &str, detail: serde_json::Value, f: impl FnOnce(&mut Board) -> T) -> T {
        let out = {
            let mut b = self.board.write();
            b.version += 1;
            f(&mut b)
        };
        self.emit(what, detail);
        out
    }

    pub fn snapshot(&self) -> Board {
        self.board.read().clone()
    }

    pub fn run(&self) -> RunId {
        self.run
    }

    /// Subdoel toevoegen; geeft het ID.
    pub fn push_goal(&self, text: &str, parent: Option<usize>, depends_on: Vec<usize>) -> usize {
        self.write("goal_added", serde_json::json!({"text": text, "parent": parent}), |b| {
            let id = b.goals.len();
            b.goals.push(Goal { id, text: text.into(), parent, depends_on, status: GoalStatus::Open, result: None });
            id
        })
    }

    pub fn set_goal_status(&self, id: usize, status: GoalStatus, result: Option<String>) {
        self.write("goal_status", serde_json::json!({"id": id, "status": status}), |b| {
            if let Some(g) = b.goals.get_mut(id) {
                g.status = status;
                if result.is_some() {
                    g.result = result;
                }
            }
        });
    }

    /// Dead-task elimination: doel laten vallen → alle subdoelen en alles wat ervan afhangt ook.
    pub fn drop_goal(&self, id: usize) -> Vec<usize> {
        self.write("goal_dropped", serde_json::json!({"id": id}), |b| {
            let mut gone = vec![id];
            loop {
                let more: Vec<usize> = b
                    .goals
                    .iter()
                    .filter(|g| {
                        !gone.contains(&g.id)
                            && (g.parent.is_some_and(|p| gone.contains(&p))
                                || g.depends_on.iter().any(|d| gone.contains(d)))
                    })
                    .map(|g| g.id)
                    .collect();
                if more.is_empty() {
                    break;
                }
                gone.extend(more);
            }
            for g in b.goals.iter_mut().filter(|g| gone.contains(&g.id) && g.status != GoalStatus::Done) {
                g.status = GoalStatus::Dropped;
            }
            gone
        })
    }

    /// Volgende uitvoerbare doelen (open, afhankelijkheden klaar).
    pub fn ready_goals(&self) -> Vec<Goal> {
        let b = self.board.read();
        b.goals
            .iter()
            .filter(|g| {
                g.status == GoalStatus::Open
                    && g.depends_on.iter().all(|d| b.goals.get(*d).is_some_and(|x| x.status == GoalStatus::Done))
            })
            .cloned()
            .collect()
    }

    pub fn add_hypothesis(&self, text: &str, confidence: f64) {
        self.write("hypothesis", serde_json::json!({"text": text, "confidence": confidence}), |b| {
            b.hypotheses.push(Hypothesis { text: text.into(), confidence })
        });
    }
    pub fn add_question(&self, q: &str) {
        self.write("open_question", serde_json::json!({"q": q}), |b| b.open_questions.push(q.into()));
    }
    pub fn add_uncertainty(&self, u: &str) {
        self.write("uncertainty", serde_json::json!({"u": u}), |b| b.uncertainties.push(u.into()));
    }
    pub fn add_note(&self, n: &str) {
        self.write("note", serde_json::json!({"n": n}), |b| b.notes.push(n.into()));
    }
    pub fn set_memories(&self, ids: Vec<String>) {
        self.write("memories", serde_json::json!({"n": ids.len()}), |b| b.memories = ids);
    }
    pub fn set_strategy(&self, s: serde_json::Value) {
        self.write("strategy", s.clone(), |b| b.strategy = Some(s));
    }
    /// Bijsturing van de gebruiker tijdens de run (W07.10).
    pub fn steer(&self, text: &str) {
        self.write("steer", serde_json::json!({"text": text}), |b| b.steering.push(text.into()));
    }
    pub fn take_steering(&self) -> Vec<String> {
        let mut b = self.board.write();
        std::mem::take(&mut b.steering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_stack_with_dependencies_and_dead_task_elimination() {
        let ws = Workspace::new(RunId::new(), TraceId::new(), "rapport", None);
        let a = ws.push_goal("bronnen zoeken", None, vec![]);
        let b = ws.push_goal("bronnen lezen", None, vec![a]);
        let c = ws.push_goal("samenvatten", None, vec![b]);
        let d = ws.push_goal("plaatjes maken", None, vec![]);
        let sub = ws.push_goal("plaatje 1", Some(d), vec![]);
        assert_eq!(ws.ready_goals().iter().map(|g| g.id).collect::<Vec<_>>(), vec![a, d, sub]);
        ws.set_goal_status(a, GoalStatus::Done, Some("3 bronnen".into()));
        assert!(ws.ready_goals().iter().any(|g| g.id == b));
        // Gebruiker: "geen plaatjes" → plaatjes + subdoel vervallen.
        let gone = ws.drop_goal(d);
        assert!(gone.contains(&sub));
        assert!(ws.ready_goals().iter().all(|g| g.id != sub && g.id != d));
        assert_eq!(ws.snapshot().goals[c].status, GoalStatus::Open);
        ws.steer("korter graag");
        assert_eq!(ws.take_steering(), vec!["korter graag".to_string()]);
        assert!(ws.take_steering().is_empty());
        assert!(ws.snapshot().version > 5);
    }
}
