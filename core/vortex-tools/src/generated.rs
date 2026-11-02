//! Zelfgeschreven tools (W18.05): Vortex schrijft een tool (Python `run(args)`), met schema en
//! tests; de tests draaien in de sandbox en alleen bij groen wordt de tool vastgelegd
//! (`config/tools/generated/<naam>/`) en geregistreerd. Bij opstarten worden ze weer geladen.
//! Alleen effect Read/Local (geen externe bijwerkingen zonder idempotentie-ledger).
// VX:W18.05

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::registry::ToolRegistry;
use crate::sandbox::Sandbox;
use crate::tool::{Tool, ToolCtx, ToolOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTest {
    pub input: Value,
    /// Uitvoer (als JSON-tekst) moet dit bevatten.
    pub expect_contains: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpecFile {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub tests: Vec<ToolTest>,
    #[serde(default)]
    pub version: u32,
}

fn harness(code: &str, args: &Value) -> String {
    format!(
        "import json as _j\n{code}\n_args = _j.loads({:?})\n_r = run(_args)\nprint('\\n__VX_RESULT__' + _j.dumps(_r, ensure_ascii=False, default=str))\n",
        args.to_string()
    )
}

async fn run_code(sb: &Sandbox, code: &str, args: &Value, ctx: Option<&ToolCtx>) -> Result<Value, ToolError> {
    let (run, trace, cancel) = match ctx {
        Some(c) => (c.run, c.trace, c.cancel.clone()),
        None => (None, vortex_types::TraceId::new(), tokio_util::sync::CancellationToken::new()),
    };
    let r = sb.python(&harness(code, args), None, run, trace, cancel).await?;
    if r.exit_code != Some(0) {
        return Err(ToolError::failed(r.text()).with_hint("traceback hierboven"));
    }
    let text = r.text();
    let line = text.rsplit("__VX_RESULT__").next().unwrap_or("").lines().next().unwrap_or("").trim();
    serde_json::from_str(line).map_err(|e| ToolError::failed(format!("geen JSON-resultaat ({e}): {text}")))
}

#[derive(Debug)]
pub struct GeneratedTool {
    c: ToolContract,
    code: String,
}

impl GeneratedTool {
    pub fn new(spec: &ToolSpecFile, code: String) -> Self {
        let mut c = ToolContract::new(
            &spec.name,
            &format!("{} (zelfgeschreven tool v{})", spec.description, spec.version),
            Effect::Local,
            spec.input_schema.clone(),
        );
        c = c.resources(vec![ResourceUse::Cpu]).timeout(120_000);
        c.origin = "generated".into();
        Self { c, code }
    }

    /// Alle opgeslagen tools laden.
    pub fn load_all(dir: &Path, reg: &ToolRegistry) -> usize {
        let mut n = 0;
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let (spec, code) = (e.path().join("tool.json"), e.path().join("tool.py"));
            if let (Ok(s), Ok(c)) = (std::fs::read_to_string(&spec), std::fs::read_to_string(&code))
                && let Ok(sp) = serde_json::from_str::<ToolSpecFile>(&s)
                && reg.get(&sp.name).is_none_or(|t| t.contract().origin == "generated")
            {
                reg.register(Arc::new(GeneratedTool::new(&sp, c)));
                n += 1;
            }
        }
        n
    }
}

#[async_trait::async_trait]
impl Tool for GeneratedTool {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let v = run_code(&ctx.sandbox, &self.code, &i, Some(ctx)).await?;
        let text = match &v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        Ok(ToolOutput { data: v, text, preview: None })
    }
}

/// `create_tool`: schrijven → testen → (bij groen) vastleggen + registreren.
#[derive(Debug)]
pub struct CreateTool {
    c: ToolContract,
    reg: Weak<ToolRegistry>,
    dir: PathBuf,
}

impl CreateTool {
    pub fn new(reg: Weak<ToolRegistry>, dir: PathBuf) -> Self {
        Self {
            c: ToolContract::new(
                "create_tool",
                "Maak een nieuwe herbruikbare tool: Python-functie `def run(args: dict)` (geeft dict/lijst/tekst terug), invoerschema en minstens 2 tests. Alleen bij geslaagde tests wordt hij geregistreerd (ook na herstart). Bestaande zelfgeschreven tool met dezelfde naam = nieuwe versie.",
                Effect::Local,
                json!({"type": "object", "required": ["name", "description", "input_schema", "code", "tests"], "properties": {
                    "name": {"type": "string", "pattern": "^[a-z][a-z0-9_]{2,40}$"},
                    "description": {"type": "string", "minLength": 10},
                    "input_schema": {"type": "object"},
                    "code": {"type": "string", "minLength": 10},
                    "tests": {"type": "array", "minItems": 2, "items": {"type": "object", "required": ["input", "expect_contains"]}}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(300_000),
            reg,
            dir,
        }
    }
}

#[async_trait::async_trait]
impl Tool for CreateTool {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let reg = self.reg.upgrade().ok_or_else(|| ToolError::failed("registry weg"))?;
        let name = i["name"].as_str().unwrap_or("").to_string();
        if reg.get(&name).is_some_and(|t| t.contract().origin != "generated") {
            return Err(ToolError::invalid(format!("'{name}' is een ingebouwde tool: kies een andere naam")));
        }
        let code = i["code"].as_str().unwrap_or("").to_string();
        let tests: Vec<ToolTest> =
            serde_json::from_value(i["tests"].clone()).map_err(|e| ToolError::invalid(format!("tests: {e}")))?;
        let mut report = Vec::new();
        let mut all_ok = true;
        for (k, t) in tests.iter().enumerate() {
            let r = run_code(&ctx.sandbox, &code, &t.input, Some(ctx)).await;
            let (ok, got) = match r {
                Ok(v) => {
                    let s = v.to_string();
                    (s.contains(&t.expect_contains) || v.as_str().is_some_and(|x| x.contains(&t.expect_contains)), s)
                }
                Err(e) => (false, e.message),
            };
            all_ok &= ok;
            report.push(json!({"test": k, "ok": ok, "got": got.chars().take(500).collect::<String>(), "expected": t.expect_contains}));
        }
        if !all_ok {
            return Err(ToolError::failed(format!(
                "tests mislukt — niet geregistreerd: {}",
                serde_json::to_string(&report).unwrap_or_default()
            ))
            .with_hint("verbeter de code of de tests en probeer opnieuw"));
        }
        let old = std::fs::read_to_string(self.dir.join(&name).join("tool.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<ToolSpecFile>(&s).ok());
        let spec = ToolSpecFile {
            name: name.clone(),
            description: i["description"].as_str().unwrap_or("").into(),
            input_schema: i["input_schema"].clone(),
            tests,
            version: old.map_or(1, |o| o.version + 1),
        };
        let d = self.dir.join(&name);
        std::fs::create_dir_all(&d)?;
        std::fs::write(d.join("tool.py"), &code)?;
        std::fs::write(d.join("tool.json"), serde_json::to_string_pretty(&spec).unwrap_or_default())?;
        reg.register(Arc::new(GeneratedTool::new(&spec, code)));
        Ok(ToolOutput::json(json!({"registered": name, "version": spec.version, "tests": report})))
    }
}
