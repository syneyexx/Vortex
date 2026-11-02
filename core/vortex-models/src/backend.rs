//! De `Backend`-trait en de berichttypen. Elke inference-backend (nu: OpenAI-compatibele
//! `llama-server`; later eigen bindings) implementeert dit. Nooit stilzwijgend doen alsof:
//! wat een backend niet kan, staat níet in [`Capabilities`] en geeft `Unsupported`.
// VX:W02.02

use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::Result;
use crate::metrics::InferenceMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Eén toolaanroep (gevalideerd).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(c: impl Into<String>) -> Self {
        Self { role: Role::System, content: c.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Self { role: Role::User, content: c.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: c.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn tool_result(call_id: impl Into<String>, name: impl Into<String>, c: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: c.into(),
            tool_calls: vec![],
            tool_call_id: Some(call_id.into()),
            name: Some(name.into()),
        }
    }
}

/// Tooldefinitie zoals het model hem ziet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON-schema van de argumenten.
    pub parameters: serde_json::Value,
}

/// Gestructureerde uitvoer afdwingen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        schema: serde_json::Value,
    },
    /// GBNF-grammatica (llama.cpp).
    Grammar {
        gbnf: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default)]
    pub response_format: Option<ResponseFormat>,
    /// gpt-oss: "low" | "medium" | "high".
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub logprobs: bool,
}

impl ChatRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stop: vec![],
            response_format: None,
            reasoning_effort: None,
            seed: None,
            logprobs: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    /// Redeneertekst (gpt-oss "reasoning_content"), apart van het antwoord.
    #[serde(default)]
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: TokenUsage,
    pub metrics: InferenceMetrics,
}

/// Gebeurtenissen tijdens streamen.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Text(String),
    Reasoning(String),
    /// Toolaanroep wordt opgesteld (nog níet uitvoeren).
    ToolCallStreaming {
        index: usize,
        name: Option<String>,
    },
    /// Toolaanroep volledig en geldig — mag uitgevoerd worden.
    ToolCallReady(ToolCall),
    Done {
        finish_reason: String,
        usage: TokenUsage,
        metrics: InferenceMetrics,
    },
}

/// Wat een backend/model echt kan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub chat: bool,
    pub streaming: bool,
    pub tools: bool,
    pub vision: bool,
    pub embeddings: bool,
    pub rerank: bool,
    pub cancellation: bool,
    pub logprobs: bool,
    pub adapters: bool,
    pub json_schema: bool,
    pub grammar: bool,
    pub kv_slots: bool,
    pub reasoning_effort: bool,
}

impl Capabilities {
    /// Alleen wat beide zeggen te kunnen (config ∩ backend).
    pub fn intersect(&self, o: &Capabilities) -> Capabilities {
        Capabilities {
            chat: self.chat && o.chat,
            streaming: self.streaming && o.streaming,
            tools: self.tools && o.tools,
            vision: self.vision && o.vision,
            embeddings: self.embeddings && o.embeddings,
            rerank: self.rerank && o.rerank,
            cancellation: self.cancellation && o.cancellation,
            logprobs: self.logprobs && o.logprobs,
            adapters: self.adapters && o.adapters,
            json_schema: self.json_schema && o.json_schema,
            grammar: self.grammar && o.grammar,
            kv_slots: self.kv_slots && o.kv_slots,
            reasoning_effort: self.reasoning_effort && o.reasoning_effort,
        }
    }

    /// Controleer een verzoek vóór verzenden. Niet ondersteund = fout, nooit negeren.
    pub fn check(&self, req: &ChatRequest) -> Result<()> {
        use crate::error::ModelError::Unsupported;
        if !self.chat {
            return Err(Unsupported("chat".into()));
        }
        if !req.tools.is_empty() && !self.tools {
            return Err(Unsupported("tools".into()));
        }
        if req.logprobs && !self.logprobs {
            return Err(Unsupported("logprobs".into()));
        }
        match &req.response_format {
            Some(ResponseFormat::JsonSchema { .. } | ResponseFormat::JsonObject) if !self.json_schema => {
                return Err(Unsupported("json_schema".into()));
            }
            Some(ResponseFormat::Grammar { .. }) if !self.grammar => return Err(Unsupported("grammar".into())),
            _ => {}
        }
        if req.reasoning_effort.is_some() && !self.reasoning_effort {
            return Err(Unsupported("reasoning_effort".into()));
        }
        Ok(())
    }
}

/// Eén rerank-score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

/// Een inference-backend.
#[async_trait::async_trait]
pub trait Backend: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn capabilities(&self) -> &Capabilities;
    async fn health(&self) -> Result<bool>;
    async fn chat(&self, req: &ChatRequest, cancel: &CancellationToken) -> Result<ChatResponse>;
    async fn chat_stream(
        &self,
        req: &ChatRequest,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>>;
    async fn embed(&self, texts: &[String], cancel: &CancellationToken) -> Result<Vec<Vec<f32>>>;
    async fn rerank(&self, query: &str, docs: &[String], cancel: &CancellationToken) -> Result<Vec<RerankScore>>;
}
