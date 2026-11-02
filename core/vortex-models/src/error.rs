//! Fouten van de modellaag.

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("serialisatie: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("backend gaf fout {status}: {body}")]
    Backend { status: u16, body: String },
    #[error("niet ondersteund door dit model/backend: {0}")]
    Unsupported(String),
    #[error("ongeldige toolaanroep ({parser}): {reason}")]
    InvalidToolCall { parser: String, reason: String },
    #[error("ongeldige modeluitvoer: {0}")]
    InvalidOutput(String),
    #[error("numerieke fout (NaN/inf): {0}")]
    Numeric(String),
    #[error("registry: {0}")]
    Registry(String),
    #[error("lifecycle: {0}")]
    Lifecycle(String),
    #[error("geannuleerd")]
    Cancelled,
    #[error("kern: {0}")]
    Core(#[from] vortex_core::CoreError),
}

pub type Result<T, E = ModelError> = std::result::Result<T, E>;
