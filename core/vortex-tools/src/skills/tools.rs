//! Skill-tools: `load_skill` (progressive disclosure) en `run_skill_script` (sandbox).

use std::sync::Arc;

use serde_json::json;

use super::SkillStore;
use crate::builtin::s;
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::tool::{Tool, ToolCtx, ToolOutput};

#[derive(Debug)]
pub struct LoadSkill {
    c: ToolContract,
    store: Arc<SkillStore>,
}
impl LoadSkill {
    pub fn new(store: Arc<SkillStore>) -> Self {
        Self {
            c: ToolContract::new(
                "load_skill",
                "Laad de volledige instructies van een skill uit de lijst (plus scripts en bestanden).",
                Effect::Read,
                json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}),
            )
            .timeout(5_000),
            store,
        }
    }
}
#[async_trait::async_trait]
impl Tool for LoadSkill {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let name = s(&i, "name");
        let sk = self.store.get(name).ok_or_else(|| {
            let names: Vec<String> = self.store.list().into_iter().map(|s| s.name).collect();
            ToolError::new(ToolErrorKind::NotFound, format!("skill '{name}' bestaat niet"))
                .with_hint(format!("beschikbaar: {}", names.join(", ")))
        })?;
        self.store.record_use(&sk.name);
        let mut text = format!("# Skill: {}\n{}\n\n{}", sk.name, sk.description, sk.body.trim());
        if !sk.scripts.is_empty() {
            text.push_str(&format!("\n\nScripts (uitvoeren met run_skill_script): {}", sk.scripts.join(", ")));
        }
        if !sk.resources.is_empty()
            && let Some(d) = &sk.dir
        {
            {
                text.push_str(&format!(
                    "\nBestanden (lezen met read_file): {}",
                    sk.resources
                        .iter()
                        .map(|r| d.join("resources").join(r).display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        Ok(ToolOutput::json(json!({"name": sk.name, "scripts": sk.scripts, "resources": sk.resources})).with_text(text))
    }
}

#[derive(Debug)]
pub struct RunSkillScript {
    c: ToolContract,
    store: Arc<SkillStore>,
}
impl RunSkillScript {
    pub fn new(store: Arc<SkillStore>) -> Self {
        Self {
            c: ToolContract::new(
                "run_skill_script",
                "Voer een script uit een skill uit (sandbox; .py/.sh/.ps1/.js) met argumenten.",
                Effect::Local,
                json!({"type": "object", "required": ["skill", "script"], "properties": {
                    "skill": {"type": "string"}, "script": {"type": "string"},
                    "args": {"type": "array", "items": {"type": "string"}, "default": []},
                    "cwd": {"type": "string"}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(600_000),
            store,
        }
    }
}
#[async_trait::async_trait]
impl Tool for RunSkillScript {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let sk = self
            .store
            .get(s(&i, "skill"))
            .ok_or_else(|| ToolError::new(ToolErrorKind::NotFound, "skill bestaat niet"))?;
        let script = s(&i, "script");
        if !sk.scripts.iter().any(|x| x == script) {
            return Err(ToolError::new(
                ToolErrorKind::Precondition,
                format!("script {script} hoort niet bij {}", sk.name),
            )
            .with_hint(format!("scripts: {}", sk.scripts.join(", "))));
        }
        let dir = sk.dir.clone().ok_or_else(|| ToolError::failed("skill heeft geen map"))?;
        let args: Vec<String> = i
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let cwd = match i.get("cwd").and_then(|c| c.as_str()) {
            Some(c) => Some(ctx.caps.resolve(c)?),
            None => Some(ctx.caps.cwd.clone()),
        };
        let r = ctx
            .sandbox
            .script(&dir.join("scripts").join(script), args, cwd, ctx.run, ctx.trace, ctx.cancel.clone())
            .await?;
        let text = r.text();
        if r.exit_code != Some(0) {
            return Err(ToolError::failed(text));
        }
        Ok(ToolOutput { data: serde_json::to_value(&r).unwrap_or_default(), text, preview: None })
    }
}
