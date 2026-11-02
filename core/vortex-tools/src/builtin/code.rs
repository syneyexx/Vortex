//! Code interpreter (Python) en shell, in de sandbox (W08.06/W08.07).

use std::sync::Arc;

use serde_json::json;

use super::{s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::sandbox::Limits;
use crate::tool::{Tool, ToolCtx, ToolOutput};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Python::new()), Arc::new(Shell::new())]
}

fn limits(ctx: &ToolCtx, i: &serde_json::Value) -> Limits {
    let mut l = ctx.sandbox.limits.clone();
    if let Some(t) = u(i, "timeout_s") {
        l.timeout_ms = t.clamp(1, 3600) * 1000;
        l.max_cpu_secs = l.max_cpu_secs.max(t);
    }
    if let Some(m) = u(i, "memory_mb") {
        l.max_memory_mb = m.clamp(64, 65_536);
    }
    l
}

fn report(r: crate::sandbox::RunReport) -> Result<ToolOutput, ToolError> {
    let text = r.text();
    let data = serde_json::to_value(&r).unwrap_or_default();
    if r.timed_out {
        return Err(
            ToolError::new(ToolErrorKind::Timeout, text).with_hint("maak de berekening kleiner of verhoog timeout_s")
        );
    }
    if r.exit_code != Some(0) {
        // Fout in de code: model krijgt stderr en kan het herstellen.
        return Err(ToolError::failed(text).with_hint("lees de traceback, pas de code aan en probeer opnieuw"));
    }
    Ok(ToolOutput { data, text, preview: None })
}

#[derive(Debug)]
pub struct Python {
    c: ToolContract,
}
impl Python {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "python",
                "Voer Python-code uit in de sandbox (eigen werkmap, tijd- en geheugenlimiet); print het resultaat.",
                Effect::Local,
                json!({"type": "object", "required": ["code"], "properties": {
                    "code": {"type": "string", "minLength": 1},
                    "timeout_s": {"type": "integer", "minimum": 1, "maximum": 3600},
                    "memory_mb": {"type": "integer", "minimum": 64}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(3_700_000),
        }
    }
}
impl Default for Python {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for Python {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        report(ctx.sandbox.python(s(&i, "code"), Some(limits(ctx, &i)), ctx.run, ctx.trace, ctx.cancel.clone()).await?)
    }
}

#[derive(Debug)]
pub struct Shell {
    c: ToolContract,
}
impl Shell {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "shell",
                "Voer een shell-commando uit (Linux: sh, Windows: PowerShell) in de sandbox; cwd optioneel.",
                Effect::Local,
                json!({"type": "object", "required": ["command"], "properties": {
                    "command": {"type": "string", "minLength": 1},
                    "cwd": {"type": "string"},
                    "timeout_s": {"type": "integer", "minimum": 1, "maximum": 3600}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(3_700_000),
        }
    }
}
impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for Shell {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let cwd = match i.get("cwd").and_then(|c| c.as_str()) {
            Some(c) => Some(ctx.caps.resolve(c)?),
            None if !ctx.caps.roots.is_empty() => Some(ctx.caps.cwd.clone()),
            None => None,
        };
        report(
            ctx.sandbox
                .shell(s(&i, "command"), cwd, Some(limits(ctx, &i)), ctx.run, ctx.trace, ctx.cancel.clone())
                .await?,
        )
    }
}
