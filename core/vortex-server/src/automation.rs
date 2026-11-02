//! Browserautomatisering en computer use als tools (W17.14), uitgevoerd door de Python-worker.
//! `browser`: stappen (goto/click/fill/press/wait/extract/screenshot/eval) in headless Chromium.
//! `computer`: schermafbeelding, klikken, typen, toetsen, scrollen op de eigen PC (bureaublad).
// VX:W17.14

use serde_json::{Value, json};
use vortex_core::Kernel;
use vortex_tools::contract::{Effect, ResourceUse, ToolContract};
use vortex_tools::{Tool, ToolCtx, ToolError, ToolErrorKind, ToolOutput};

async fn job(kernel: &Kernel, kind: &str, mut cfg: Value) -> Result<Value, ToolError> {
    let dir =
        kernel.db().path().and_then(|p| p.parent()).map(|p| p.join("automation")).unwrap_or_else(std::env::temp_dir);
    let id = vortex_types::EventId::new().to_string();
    cfg["result"] = json!(dir.join(format!("{id}.json")).display().to_string());
    if cfg.get("out_dir").is_none() {
        cfg["out_dir"] = json!(dir.join(&id).display().to_string());
    }
    let (p, _) = crate::world_api::worker_job(kernel, kind, crate::world_api::toml_kv(&cfg))
        .await
        .map_err(|e| ToolError::new(ToolErrorKind::Transient, e))?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| ToolError::failed(e.to_string()))?)
        .map_err(|e| ToolError::failed(e.to_string()))?;
    Ok(v)
}

#[derive(Debug)]
pub struct Browser {
    c: ToolContract,
    kernel: Kernel,
}

impl Browser {
    pub fn new(kernel: Kernel) -> Self {
        Self {
            c: ToolContract::new(
                "browser",
                "Bedien een browser (ook JS-sites, formulieren): stappen [{action: goto|click|fill|press|wait|extract|screenshot|eval, url, selector, text, key, ms, js}]. Geeft per stap het resultaat (extract → tekst, screenshot → pad).",
                Effect::External,
                json!({"type": "object", "required": ["steps"], "properties": {
                    "steps": {"type": "array", "minItems": 1, "items": {"type": "object", "required": ["action"]}},
                    "headless": {"type": "boolean"}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(300_000),
            kernel,
        }
    }
}

#[async_trait::async_trait]
impl Tool for Browser {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        let browser =
            self.kernel.config().current().get::<String>("paths.browser").ok().flatten().filter(|s| !s.is_empty());
        let v = job(&self.kernel, "browser:run", json!({"steps": i["steps"].to_string(), "headless": i.get("headless").and_then(|b| b.as_bool()).unwrap_or(true), "executable": browser})).await?;
        let mut text = format!("Pagina: {} ({})\n", v["title"].as_str().unwrap_or(""), v["url"].as_str().unwrap_or(""));
        for r in v["results"].as_array().into_iter().flatten() {
            text.push_str(&format!(
                "[{}] {} {}{}\n",
                r["i"],
                r["action"].as_str().unwrap_or(""),
                if r["ok"] == true { "ok" } else { "MISLUKT" },
                r["text"]
                    .as_str()
                    .map(|t| format!(": {}", t.chars().take(4000).collect::<String>()))
                    .or_else(|| r["error"].as_str().map(|e| format!(": {e}")))
                    .or_else(|| r["path"].as_str().map(|p| format!(": {p}")))
                    .unwrap_or_default()
            ));
        }
        let failed = v["results"].as_array().is_some_and(|a| a.iter().any(|r| r["ok"] == false));
        if failed {
            return Err(ToolError::failed(text)
                .with_hint("controleer de selector (screenshot of extract van 'body' helpt) en probeer opnieuw"));
        }
        Ok(ToolOutput { data: v, text, preview: None })
    }
}

#[derive(Debug)]
pub struct Computer {
    c: ToolContract,
    kernel: Kernel,
}

impl Computer {
    pub fn new(kernel: Kernel) -> Self {
        Self {
            c: ToolContract::new(
                "computer",
                "Bedien deze PC: screenshot (daarna met visie bekijken), click {x,y}, move {x,y}, type {text}, key {keys: \"ctrl+s\"}, scroll {dy}. Muis in een schermhoek = noodstop.",
                Effect::External,
                json!({"type": "object", "required": ["op"], "properties": {
                    "op": {"type": "string", "enum": ["screenshot", "click", "move", "type", "key", "scroll"]},
                    "x": {"type": "integer"}, "y": {"type": "integer"},
                    "text": {"type": "string"}, "keys": {"type": "string"}, "dy": {"type": "integer"}
                }}),
            )
            .timeout(60_000),
            kernel,
        }
    }
}

#[async_trait::async_trait]
impl Tool for Computer {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: Value) -> Result<ToolOutput, ToolError> {
        let op = i["op"].as_str().unwrap_or("screenshot").to_string();
        let mut cfg = i.clone();
        if op == "screenshot" {
            let dir = self
                .kernel
                .db()
                .path()
                .and_then(|p| p.parent())
                .map(|p| p.join("automation"))
                .unwrap_or_else(std::env::temp_dir);
            cfg["out"] = json!(dir.join(format!("screen_{}.png", vortex_types::EventId::new())).display().to_string());
        }
        let v = job(&self.kernel, &format!("desktop:{op}"), cfg).await?;
        let text = match op.as_str() {
            "screenshot" => {
                format!("Schermafbeelding {}×{}: {}", v["width"], v["height"], v["path"].as_str().unwrap_or(""))
            }
            _ => format!("{op}: ok"),
        };
        Ok(ToolOutput { data: v, text, preview: None })
    }
}

// ── Repo-niveau codebegrip (W17.14): codebase → geheugenwerkruimte + repokaart ──

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};

use crate::api::{ApiError, AppState};

#[derive(Debug, serde::Deserialize)]
struct RepoIndex {
    path: String,
    #[serde(default)]
    name: Option<String>,
}

/// Code inlezen in werkruimte `repo:<naam>` (zoekbaar met locators per regel) en de repokaart
/// (talen + symbolen) als projectkennis vastleggen.
async fn repo_index(State(s): State<AppState>, Json(b): Json<RepoIndex>) -> Result<Json<Value>, ApiError> {
    let root = std::path::PathBuf::from(&b.path);
    if !root.is_dir() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "geen map"));
    }
    let name = b.name.unwrap_or_else(|| root.file_name().and_then(|n| n.to_str()).unwrap_or("repo").to_string());
    let ws = format!("repo:{name}");
    let hub = s.tools.clone().ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "geen tools"))?;
    let opts = vortex_tools::ExecOptions { caps: vortex_tools::CapabilityHandle::scoped(&root), ..Default::default() };
    let map =
        hub.executor.call(&format!("repo_map:{}", vortex_types::EventId::new()), "repo_map", json!({}), &opts).await;
    let map_text = map.output.as_ref().map(|o| o.text.clone()).unwrap_or_default();
    let (_, rep) = s
        .chat
        .ingest(vec![root.display().to_string()], Some(ws.clone()), vortex_types::Priority::P2)
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    if let Some(mem) = &s.chat.memory {
        let mut m = vortex_memory::NewMemory::new(
            vortex_memory::MemoryKind::Project,
            format!("Repokaart van {name}:\n{}", map_text.chars().take(12_000).collect::<String>()),
            vortex_types::Provenance::with_origin(vortex_types::SourceKind::Tool, "repo_map"),
        );
        m.title = format!("Repokaart {name}");
        m.workspace = Some(ws.clone());
        m.source_uri = Some(root.display().to_string());
        m.source_hash = Some(format!("repomap:{}", root.display()));
        m.force = true;
        let _ = mem.store.write_or_defer(m).await;
    }
    Ok(Json(json!({"workspace": ws, "ingest": rep, "map": map.output.map(|o| o.data["languages"].clone())})))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/repo/index", post(repo_index))
}
