//! Ingebouwde tools (W08.07).

pub mod api;
pub mod code;
pub mod fs;
pub mod meta;
pub mod sqlite;
pub mod web;

use std::sync::Arc;

use crate::registry::ToolRegistry;

/// Instellingen voor de ingebouwde tools.
#[derive(Debug, Clone, Default)]
pub struct BuiltinConfig {
    /// SearXNG-basis-URL (bv. `http://127.0.0.1:8888`).
    pub searxng_url: Option<String>,
    pub user_agent: Option<String>,
}

pub fn register_all(reg: &Arc<ToolRegistry>, cfg: &BuiltinConfig) {
    for t in fs::tools() {
        reg.register(t);
    }
    for t in code::tools() {
        reg.register(t);
    }
    for t in sqlite::tools() {
        reg.register(t);
    }
    for t in web::tools(cfg) {
        reg.register(t);
    }
    reg.register(Arc::new(api::HttpGet::new(cfg)));
    reg.register(Arc::new(api::HttpRequest::new(cfg)));
    reg.register(Arc::new(meta::DescribeTool::new(Arc::downgrade(reg))));
}

/// Helper: string-veld uit de invoer.
pub(crate) fn s<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
pub(crate) fn u(v: &serde_json::Value, k: &str) -> Option<u64> {
    v.get(k).and_then(|x| x.as_u64())
}
pub(crate) fn b(v: &serde_json::Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}
