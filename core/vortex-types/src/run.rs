//! Run-statussen en prioriteiten (gedeeld met GUI en CLI). Overgangsregels staan in `vortex-core`.

use serde::{Deserialize, Serialize};

/// Status van een run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    Starting,
    Running,
    Waiting,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl RunState {
    pub const ALL: [RunState; 8] = [
        RunState::Queued,
        RunState::Starting,
        RunState::Running,
        RunState::Waiting,
        RunState::Cancelling,
        RunState::Completed,
        RunState::Failed,
        RunState::Cancelled,
    ];

    /// Eindstatus: hierna geen overgangen meer.
    pub fn is_terminal(self) -> bool {
        matches!(self, RunState::Completed | RunState::Failed | RunState::Cancelled)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Queued => "queued",
            RunState::Starting => "starting",
            RunState::Running => "running",
            RunState::Waiting => "waiting",
            RunState::Cancelling => "cancelling",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|st| st.as_str() == s)
    }
}

/// Drie fasen van stoppen (cancellation): aangevraagd → gestopt → opgeruimd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopStatus {
    Requested,
    Stopped,
    CleanedUp,
}

/// Prioriteitsklasse (zie Dirigent). Lager getal = belangrijker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// Interactief: chat, stopknop, GUI.
    P0 = 0,
    /// Gebruiker wacht.
    P1 = 1,
    /// Achtergrond met waarde.
    P2 = 2,
    /// Idle.
    P3 = 3,
}

/// Soort run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Chat,
    Research,
    Ingest,
    Training,
    Maintenance,
    Eval,
    Other(String),
}

impl RunKind {
    pub fn as_str(&self) -> &str {
        match self {
            RunKind::Chat => "chat",
            RunKind::Research => "research",
            RunKind::Ingest => "ingest",
            RunKind::Training => "training",
            RunKind::Maintenance => "maintenance",
            RunKind::Eval => "eval",
            RunKind::Other(s) => s,
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "chat" => RunKind::Chat,
            "research" => RunKind::Research,
            "ingest" => RunKind::Ingest,
            "training" => RunKind::Training,
            "maintenance" => RunKind::Maintenance,
            "eval" => RunKind::Eval,
            other => RunKind::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_str_roundtrip() {
        for s in RunState::ALL {
            assert_eq!(RunState::parse(s.as_str()), Some(s));
        }
    }
    #[test]
    fn priority_order() {
        assert!(Priority::P0 < Priority::P3);
    }
}
