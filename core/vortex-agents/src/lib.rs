//! Debatteam (W09): rollen uit `config/roles/`, dynamische teamsamenstelling, Solvers met
//! diversiteit, Fact-checker (lokaal bewijs, webcheck als online), Critic, Specialist (draait
//! code), verifier-guided kandidaatselectie, Judge-gate, Synthesizer-instructie, bewijsoverzicht,
//! conflict-team met geldigheidsperiodes en rollback.
//!
//! Instap: [`debate::run`] en [`conflict::resolve`]. Statusupdates gaan via [`Reporter`] (events
//! → Agent collaboration-paneel); het antwoord zelf streamt de server (Synthesizer).
// VX:W09.01
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod agentmem;
pub mod conflict;
pub mod connectivity;
pub mod debate;
pub mod evidence;
pub mod factcheck;
pub mod json;
pub mod records;
pub mod research;
pub mod roles;
pub mod specialist;
pub mod team;
pub mod world;

use std::sync::Arc;

use vortex_core::EventLog;
use vortex_types::{Event, EventKind, RunId, TraceId};

pub use debate::{DebateDeps, DebateOutcome};
pub use evidence::{ClaimCheck, Label};
pub use roles::{Role, Roles};
pub use team::{TeamPlan, compose};

/// Statusmeldingen van rollen (events met run-ID → GUI).
#[derive(Debug, Clone)]
pub struct Reporter {
    log: Option<EventLog>,
    run: Option<RunId>,
    trace: TraceId,
}

impl Reporter {
    pub fn new(log: Option<EventLog>, run: Option<RunId>, trace: TraceId) -> Self {
        Self { log, run, trace }
    }
    pub fn silent() -> Self {
        Self { log: None, run: None, trace: TraceId::new() }
    }
    pub fn emit(&self, kind: EventKind, payload: serde_json::Value) {
        if let Some(l) = &self.log {
            let mut e = Event::new(kind, self.trace, payload);
            e.run_id = self.run;
            let _ = l.append(e);
        }
    }
    pub fn started(&self, role: &str, detail: &str) {
        self.emit(EventKind::AgentRoleStarted, serde_json::json!({"role": role, "detail": detail}));
    }
    pub fn done(&self, role: &str, summary: &str, data: serde_json::Value) {
        self.emit(EventKind::AgentRoleCompleted, serde_json::json!({"role": role, "summary": summary, "data": data}));
    }
}

/// Geheugen + optionele reranker.
pub type MemoryAccess = (Arc<vortex_memory::MemoryStore>, Option<Arc<dyn vortex_memory::Reranker>>);

/// Gedeelde LLM-handle.
pub type Llm = Arc<dyn vortex_cognition::CogLlm>;
