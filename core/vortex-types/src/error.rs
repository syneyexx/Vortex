//! Gemeenschappelijke foutsoorten. Crates hebben eigen fouttypen, maar mappen naar `ErrorKind`
//! zodat GUI, events en crash-rapport overal hetzelfde begrip gebruiken.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidInput,
    NotFound,
    Conflict,
    Cancelled,
    DeadlineExceeded,
    BudgetExceeded,
    ResourceExhausted,
    Unavailable,
    Storage,
    Protocol,
    Internal,
}

/// Generieke fout met soort + boodschap.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("{kind:?}: {message}")]
pub struct VortexError {
    pub kind: ErrorKind,
    pub message: String,
}

impl VortexError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}
