//! De `Tool`-trait en de context waarin een tool draait.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use vortex_types::{RunId, TraceId};

use crate::caps::CapabilityHandle;
use crate::contract::ToolContract;
use crate::error::ToolError;

/// Alles wat een tool tijdens uitvoeren mag gebruiken.
#[derive(Debug, Clone)]
pub struct ToolCtx {
    pub run: Option<RunId>,
    pub trace: TraceId,
    pub cancel: CancellationToken,
    pub caps: CapabilityHandle,
    pub sandbox: Arc<crate::sandbox::Sandbox>,
    /// Transactionele modus: schrijftools zetten wijzigingen klaar i.p.v. ze direct toe te passen.
    pub txn: Option<Arc<crate::txn::FileTxn>>,
}

/// Resultaat van een tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Gestructureerd resultaat (voor ketens en postcondities).
    pub data: serde_json::Value,
    /// Tekst voor het model (kort, gefilterd).
    pub text: String,
    /// Preview/diff bij schrijfacties.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl ToolOutput {
    pub fn text(t: impl Into<String>) -> Self {
        let t = t.into();
        Self { data: serde_json::Value::String(t.clone()), text: t, preview: None }
    }
    pub fn json(data: serde_json::Value) -> Self {
        let text = match &data {
            serde_json::Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        Self { data, text, preview: None }
    }
    pub fn with_text(mut self, t: impl Into<String>) -> Self {
        self.text = t.into();
        self
    }
    pub fn with_preview(mut self, p: impl Into<String>) -> Self {
        self.preview = Some(p.into());
        self
    }
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    fn contract(&self) -> &ToolContract;
    async fn call(&self, ctx: &ToolCtx, input: serde_json::Value) -> Result<ToolOutput, ToolError>;
}
