//! Fouttype van de kern.

use vortex_types::{ErrorKind, RunState, VortexError};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("opslag: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("serialisatie: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("niet gevonden: {0}")]
    NotFound(String),
    #[error("ongeldige overgang {from:?} → {to:?}: {reason}")]
    InvalidTransition { from: RunState, to: RunState, reason: String },
    #[error("versieconflict (optimistic concurrency): verwacht {expected}, gevonden {found}")]
    VersionConflict { expected: i64, found: i64 },
    #[error("geannuleerd")]
    Cancelled,
    #[error("deadline verlopen")]
    DeadlineExceeded,
    #[error("budget overschreden: {0}")]
    BudgetExceeded(String),
    #[error("wachtrij vol: {0}")]
    QueueFull(String),
    #[error("config: {0}")]
    Config(String),
    #[error("secrets: {0}")]
    Secrets(String),
    #[error("proces: {0}")]
    Process(String),
    #[error("migratie: {0}")]
    Migration(String),
}

impl CoreError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            CoreError::Storage(_) | CoreError::Migration(_) => ErrorKind::Storage,
            CoreError::Serde(_) => ErrorKind::Protocol,
            CoreError::Io(_) | CoreError::Process(_) => ErrorKind::Unavailable,
            CoreError::NotFound(_) => ErrorKind::NotFound,
            CoreError::InvalidTransition { .. } | CoreError::Config(_) => ErrorKind::InvalidInput,
            CoreError::VersionConflict { .. } => ErrorKind::Conflict,
            CoreError::Cancelled => ErrorKind::Cancelled,
            CoreError::DeadlineExceeded => ErrorKind::DeadlineExceeded,
            CoreError::BudgetExceeded(_) => ErrorKind::BudgetExceeded,
            CoreError::QueueFull(_) => ErrorKind::ResourceExhausted,
            CoreError::Secrets(_) => ErrorKind::Internal,
        }
    }
}

impl From<CoreError> for VortexError {
    fn from(e: CoreError) -> Self {
        VortexError::new(e.kind(), e.to_string())
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
