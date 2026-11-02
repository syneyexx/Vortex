//! Wat de cognitie van een taalmodel nodig heeft. De server koppelt dit aan een backend.

use vortex_models::backend::Message;

#[derive(Debug, Clone, Default)]
pub struct GenOpts {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub json_schema: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
}

#[async_trait::async_trait]
pub trait CogLlm: Send + Sync + std::fmt::Debug {
    async fn complete(&self, messages: Vec<Message>, opts: GenOpts) -> Result<String, String>;
}
