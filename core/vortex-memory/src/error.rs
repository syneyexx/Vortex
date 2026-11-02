//! Fouten van het geheugen.

#[derive(Debug, thiserror::Error)]
pub enum MemError {
    #[error("kern: {0}")]
    Core(#[from] vortex_core::CoreError),
    #[error("opslag: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("serialisatie: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("embedding: {0}")]
    Embed(String),
    #[error("model: {0}")]
    Llm(String),
    #[error("niet gevonden: {0}")]
    NotFound(String),
    #[error("ongeldig: {0}")]
    Invalid(String),
}

pub type Result<T, E = MemError> = std::result::Result<T, E>;
