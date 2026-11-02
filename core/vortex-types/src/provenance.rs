//! Herkomstlabels. Nu alleen vastleggen (hook); policies erop komen pas in de ethics & security layer.

use serde::{Deserialize, Serialize};

/// Soort bron van een stuk data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    User,
    Model,
    Tool,
    Web,
    File,
    Dataset,
    Feed,
    Memory,
    System,
}

/// Waar data vandaan komt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: SourceKind,
    /// Vrije aanduiding: URL, pad, toolnaam, modelnaam, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Provenance {
    pub fn new(source: SourceKind) -> Self {
        Self { source, origin: None }
    }
    pub fn with_origin(source: SourceKind, origin: impl Into<String>) -> Self {
        Self { source, origin: Some(origin.into()) }
    }
    pub fn system() -> Self {
        Self::new(SourceKind::System)
    }
}
