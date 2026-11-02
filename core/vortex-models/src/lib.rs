//! # vortex-models — inference en modelbeheer
//!
//! Registry ([`registry::Registry`]), de [`backend::Backend`]-trait met de OpenAI-compatibele
//! adapter ([`openai::OpenAiBackend`], v1 = llama-server), toolcall-parsers, streaming-hulpen en
//! metingen. Zie `CARD.md`.

pub mod backend;
pub mod error;
pub mod lifecycle;
pub mod metrics;
pub mod openai;
pub mod registry;
pub mod stream;
pub mod toolcall;

pub use backend::{
    Backend, Capabilities, ChatRequest, ChatResponse, Message, ResponseFormat, Role, StreamEvent, ToolCall, ToolSpec,
};
pub use error::{ModelError, Result};
pub use lifecycle::{LaunchConfig, LoadReport, ModelServer, Phase};
pub use metrics::InferenceMetrics;
pub use openai::OpenAiBackend;
pub use registry::{ModelEntry, Registry, ServerEntry};
