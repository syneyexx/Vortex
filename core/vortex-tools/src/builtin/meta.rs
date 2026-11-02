//! Discovery-tools: `describe_tool` (schema bij gebruik) en `call_tool` (een tool aanroepen die
//! niet in de kernset met volledige schema's zit; de hub pakt dit uit vóór de executor).

use std::sync::Weak;

use serde_json::json;

use super::s;
use crate::contract::{Effect, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolCtx, ToolOutput};

#[derive(Debug)]
pub struct DescribeTool {
    c: ToolContract,
    reg: Weak<ToolRegistry>,
}
impl DescribeTool {
    pub fn new(reg: Weak<ToolRegistry>) -> Self {
        Self {
            c: ToolContract::new(
                "describe_tool",
                "Toon het invoerschema en contract van een tool uit de lijst (vóór call_tool).",
                Effect::Read,
                json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}),
            )
            .timeout(2_000),
            reg,
        }
    }
}
#[async_trait::async_trait]
impl Tool for DescribeTool {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let reg = self.reg.upgrade().ok_or_else(|| ToolError::failed("registry weg"))?;
        let name = s(&i, "name");
        match reg.contract(name) {
            Some(c) => Ok(ToolOutput::json(json!({
                "name": c.name, "description": c.description, "input_schema": c.input_schema,
                "effect": c.effect, "timeout_ms": c.timeout_ms, "origin": c.origin
            }))),
            None => Err(ToolError::new(ToolErrorKind::NotFound, format!("tool '{name}' bestaat niet"))
                .with_hint(format!("bedoelde je: {}", reg.closest(name, 3).join(", ")))),
        }
    }
}

/// Alleen het contract; uitvoering via [`crate::hub::ToolHub::unwrap_call`].
#[derive(Debug)]
pub struct CallTool {
    c: ToolContract,
}
impl CallTool {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "call_tool",
                "Roep een tool uit de lijst 'Overige tools' aan: {name, input}. Vraag eerst het schema met describe_tool.",
                Effect::Read,
                json!({"type": "object", "required": ["name", "input"], "properties": {"name": {"type": "string"}, "input": {"type": "object"}}}),
            ),
        }
    }
}
impl Default for CallTool {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for CallTool {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, _i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::failed("call_tool wordt door de hub uitgepakt"))
    }
}
