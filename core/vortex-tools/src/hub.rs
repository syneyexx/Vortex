//! `ToolHub`: registry + executor + skills + MCP + API's, gebouwd uit `config/tools.toml`.
//! De server hangt er geheugen-tools aan en gebruikt [`ToolHub::core_set`] (volledige schema's)
//! en [`ToolHub::prompt_block`] (overige tools + skills, alleen naam+beschrijving).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use vortex_core::{ConfigSnapshot, EventLog, Supervisor};
use vortex_types::{Event, EventKind, TraceId};

use crate::builtin::api::{ApiSpec, SecretFn, api_tools};
use crate::builtin::{BuiltinConfig, meta::CallTool};
use crate::executor::Executor;
use crate::mcp::{McpManager, McpServerCfg};
use crate::registry::ToolRegistry;
use crate::sandbox::{Limits, Sandbox};
use crate::skills::SkillStore;
use crate::skills::tools::{LoadSkill, RunSkillScript};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolHubConfig {
    #[serde(default)]
    pub searxng_url: Option<String>,
    #[serde(default)]
    pub python: Option<String>,
    #[serde(default)]
    pub workdir: Option<PathBuf>,
    #[serde(default)]
    pub skills_dir: Option<PathBuf>,
    #[serde(default)]
    pub limits: Option<Limits>,
    #[serde(default)]
    pub mcp: McpSection,
    #[serde(default)]
    pub apis: BTreeMap<String, ApiSpec>,
    /// Tools met volledig schema in elke prompt (de rest via describe_tool/call_tool).
    #[serde(default)]
    pub core: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpSection {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerCfg>,
}

/// Standaard-kernset (volledige schema's in de prompt).
pub const DEFAULT_CORE: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "list_dir",
    "search_files",
    "find_files",
    "python",
    "shell",
    "web_search",
    "web_fetch",
    "memory_search",
    "memory_write",
    "load_skill",
    "describe_tool",
    "call_tool",
];

impl ToolHubConfig {
    /// Uit `config/tools.toml` (tabel `tools`). Relatieve paden t.o.v. de config- of datamap.
    pub fn from_snapshot(snap: &ConfigSnapshot, config_dir: &std::path::Path, data_dir: &std::path::Path) -> Self {
        let mut c: ToolHubConfig = snap.values.get("tools").and_then(|v| v.clone().try_into().ok()).unwrap_or_default();
        c.skills_dir = Some(match c.skills_dir.take() {
            Some(p) if p.is_relative() => config_dir.join(p),
            Some(p) => p,
            None => config_dir.join("skills"),
        });
        c.workdir = Some(match c.workdir.take() {
            Some(p) if p.is_relative() => data_dir.join(p),
            Some(p) => p,
            None => data_dir.join("sandbox"),
        });
        c
    }
}

#[derive(Debug)]
pub struct ToolHub {
    pub registry: Arc<ToolRegistry>,
    pub executor: Executor,
    pub skills: Arc<SkillStore>,
    pub mcp: McpManager,
    pub cfg: ToolHubConfig,
    log: EventLog,
}

impl ToolHub {
    pub fn new(cfg: ToolHubConfig, log: EventLog, supervisor: Supervisor, secrets: SecretFn) -> Arc<Self> {
        let registry = Arc::new(ToolRegistry::new());
        let bcfg = BuiltinConfig { searxng_url: cfg.searxng_url.clone(), user_agent: None };
        crate::builtin::register_all(&registry, &bcfg);
        registry.register(Arc::new(CallTool::new()));
        for t in api_tools(&cfg.apis, &bcfg, secrets) {
            registry.register(t);
        }
        let workdir = cfg.workdir.clone().unwrap_or_else(|| std::env::temp_dir().join("vortex-sandbox"));
        let mut sb = Sandbox::new(supervisor, workdir);
        if let Some(p) = &cfg.python {
            sb.python = p.clone();
        }
        if let Some(l) = &cfg.limits {
            sb.limits = l.clone();
        }
        let skills = SkillStore::new(
            cfg.skills_dir.clone().unwrap_or_else(|| PathBuf::from("config/skills")),
            Some(log.clone()),
        );
        let _ = skills.watch();
        registry.register(Arc::new(LoadSkill::new(skills.clone())));
        registry.register(Arc::new(RunSkillScript::new(skills.clone())));
        // Zelfgeschreven tools (W18.05): naast de skills-map.
        let gen_dir = cfg
            .skills_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("config/skills"))
            .parent()
            .map(|p| p.join("tools").join("generated"))
            .unwrap_or_else(|| PathBuf::from("config/tools/generated"));
        crate::generated::GeneratedTool::load_all(&gen_dir, &registry);
        registry.register(Arc::new(crate::generated::CreateTool::new(Arc::downgrade(&registry), gen_dir)));
        let executor = Executor::new(registry.clone(), log.clone(), Arc::new(sb));
        Arc::new(Self { registry, executor, skills, mcp: McpManager::new(), cfg, log })
    }

    /// MCP-servers verbinden (async; mag op de achtergrond).
    pub async fn connect_mcp(&self) -> BTreeMap<String, String> {
        let st = self.mcp.connect_all(&self.cfg.mcp.servers, &self.registry).await;
        let _ =
            self.log.append(Event::new(EventKind::McpConnected, TraceId::new(), serde_json::json!({"servers": st})));
        st
    }

    /// Kernset: deze tools krijgt het model met volledig schema.
    pub fn core_set(&self) -> Vec<String> {
        let want: Vec<String> = if self.cfg.core.is_empty() {
            DEFAULT_CORE.iter().map(|s| s.to_string()).collect()
        } else {
            self.cfg.core.clone()
        };
        want.into_iter().filter(|n| self.registry.get(n).is_some()).collect()
    }

    /// (naam, beschrijving, schema) voor de kernset.
    pub fn specs(&self) -> Vec<(String, String, serde_json::Value)> {
        self.core_set()
            .into_iter()
            .filter_map(|n| self.registry.contract(&n))
            .map(|c| (c.name, c.description, c.input_schema))
            .collect()
    }

    /// Prompt-blok: overige tools (discovery) + skills (progressive disclosure).
    pub fn prompt_block(&self) -> String {
        let core = self.core_set();
        let mut out = String::new();
        let rest = self.registry.briefs_text(&core);
        if !rest.is_empty() {
            out.push_str("Overige tools (schema via describe_tool, aanroepen via call_tool):\n");
            out.push_str(&rest);
            out.push('\n');
        }
        let sk = self.skills.briefs_text();
        if !sk.is_empty() {
            out.push_str("\nSkills (laad de instructies met load_skill als er een past):\n");
            out.push_str(&sk);
            out.push('\n');
        }
        out
    }

    /// `call_tool{name, input}` → echte aanroep.
    pub fn unwrap_call(&self, name: &str, args: serde_json::Value) -> (String, serde_json::Value) {
        if name == "call_tool"
            && let Some(n) = args.get("name").and_then(|n| n.as_str())
        {
            let input = args.get("input").cloned().unwrap_or(serde_json::json!({}));
            return (n.to_string(), input);
        }
        (name.to_string(), args)
    }
}
