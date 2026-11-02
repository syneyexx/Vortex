//! De cognitieve cyclus (W07.01): 8 expliciete fasen, elk met duur + samenvatting als event.
// VX:W07.01

use std::time::Instant;

use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, RunId, TraceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Perceive,
    Attend,
    Recall,
    Deliberate,
    Act,
    Verify,
    Reflect,
    Consolidate,
}

impl Phase {
    pub const ALL: [Phase; 8] = [
        Phase::Perceive,
        Phase::Attend,
        Phase::Recall,
        Phase::Deliberate,
        Phase::Act,
        Phase::Verify,
        Phase::Reflect,
        Phase::Consolidate,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Phase::Perceive => "Waarnemen",
            Phase::Attend => "Aandacht",
            Phase::Recall => "Herinneren",
            Phase::Deliberate => "Overwegen",
            Phase::Act => "Handelen",
            Phase::Verify => "Verifiëren",
            Phase::Reflect => "Reflecteren",
            Phase::Consolidate => "Consolideren",
        }
    }
}

/// Houdt de fasen van één run bij.
#[derive(Debug)]
pub struct Cycle {
    run: RunId,
    trace: TraceId,
    log: EventLog,
    cur: Option<(Phase, Instant)>,
    done: Vec<Phase>,
}

impl Cycle {
    pub fn new(run: RunId, trace: TraceId, log: EventLog) -> Self {
        Self { run, trace, log, cur: None, done: Vec::new() }
    }

    /// Begin fase `p` (sluit de vorige af).
    pub fn enter(&mut self, p: Phase) {
        self.finish(serde_json::json!({}));
        self.cur = Some((p, Instant::now()));
    }

    /// Sluit de huidige fase af met een samenvatting.
    pub fn finish(&mut self, summary: serde_json::Value) {
        if let Some((p, t)) = self.cur.take() {
            let _ = self.log.append(
                Event::new(
                    EventKind::PhaseCompleted,
                    self.trace,
                    serde_json::json!({"phase": p, "label": p.label(), "ms": t.elapsed().as_millis() as u64, "summary": summary}),
                )
                .for_run(self.run),
            );
            self.done.push(p);
        }
    }

    pub fn completed(&self) -> &[Phase] {
        &self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vortex_core::{Db, db::CORE_MIGRATIONS};

    #[test]
    fn all_phases_logged_in_order() {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap_or_else(|e| panic!("{e}"))));
        let run = RunId::new();
        let mut c = Cycle::new(run, TraceId::new(), log.clone());
        for p in Phase::ALL {
            c.enter(p);
        }
        c.finish(serde_json::json!({"ok": true}));
        assert_eq!(c.completed(), &Phase::ALL);
        let ev = log.for_run(run).unwrap_or_default();
        assert_eq!(ev.iter().filter(|e| e.kind == EventKind::PhaseCompleted).count(), 8);
        assert_eq!(ev.last().map(|e| e.payload["phase"].clone()), Some(serde_json::json!("consolidate")));
    }
}
