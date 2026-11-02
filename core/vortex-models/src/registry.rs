//! Model-registry uit `config/models.toml` (hot-reload via de config-manager van de kern).
//! Per model: bestand, rollen, GPU, VRAM, context, KV-type, chat-template, toolcall-parser,
//! capabilities, gemeten laadtijd, licentie. Validatie geeft álle fouten in één keer.
// VX:W02.01

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vortex_core::ConfigSnapshot;

use crate::backend::Capabilities;
use crate::error::{ModelError, Result};
use crate::toolcall::parser_by_id;

/// Een llama-server-proces (één per GPU).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerEntry {
    pub url: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub gpu: Option<u32>,
    /// Model dat standaard op deze server draait.
    #[serde(default)]
    pub model: Option<String>,
    /// VRAM van deze GPU (MiB), voor admission control.
    #[serde(default)]
    pub vram_mib: Option<u64>,
    /// Reservering voor bureaublad/drivers (MiB).
    #[serde(default)]
    pub reserve_mib: Option<u64>,
    /// Vortex start/stopt deze llama-server zelf (W15.01: GPU vrijgeven bij gamen).
    #[serde(default)]
    pub managed: bool,
    /// Extra programma-argumenten vóór de modelargumenten (wrapper of test-dubbel).
    #[serde(default)]
    pub launch_args: Vec<String>,
}

/// Eén LoRA-adapter bij een model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoraEntry {
    /// Naam (adapter-ID in de registry).
    pub name: String,
    /// GGUF-adapterbestand (convert_lora_to_gguf.py).
    pub path: String,
    #[serde(default)]
    pub scale: f32,
}

/// Eén model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub file: String,
    pub roles: Vec<String>,
    /// Naam van de server (`main`/`aux`).
    pub gpu: String,
    #[serde(default)]
    pub context: Option<u32>,
    #[serde(default)]
    pub kv_type: Option<String>,
    #[serde(default)]
    pub flash_attention: Option<bool>,
    /// Geschatte VRAM voor gewichten (MiB).
    #[serde(default)]
    pub vram_weights_mib: Option<u64>,
    /// "jinja" (template uit GGUF) of een bestandsnaam.
    #[serde(default)]
    pub chat_template: Option<String>,
    #[serde(default = "default_parser")]
    pub toolcall_parser: String,
    #[serde(default)]
    pub capabilities: Option<Capabilities>,
    #[serde(default)]
    pub load_ms_measured: Option<u64>,
    #[serde(default)]
    pub license: Option<String>,
    /// Embedding-dimensie (MRL).
    #[serde(default)]
    pub dims: Option<u32>,
    /// Extra llama-server-argumenten.
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// LoRA-adapters (W13.04): bij start geladen met schaal 0, hot-swappen via `/lora-adapters`.
    #[serde(default)]
    pub loras: Vec<LoraEntry>,
    /// Vision-projector (llama-server `--mmproj`) voor beeldinvoer (W16.01).
    #[serde(default)]
    pub mmproj: Option<String>,
    /// Speculative decoding (W17.08): klein draft-model + gemeten versnelling. Alleen gebruikt als
    /// `spec_gain` (tok/s met ÷ zonder, uit `vortex specbench`) ≥ 1,05.
    #[serde(default)]
    pub draft: Option<String>,
    #[serde(default)]
    pub draft_max: Option<u32>,
    #[serde(default)]
    pub spec_gain: Option<f64>,
}

fn default_parser() -> String {
    "openai-native/v1".into()
}

/// De registry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Registry {
    pub default: String,
    #[serde(default)]
    pub servers: BTreeMap<String, ServerEntry>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelEntry>,
    /// Config-versie waaruit deze registry komt.
    #[serde(skip)]
    pub config_version: String,
}

const KNOWN_ROLES: &[&str] = &[
    "chat",
    "agents",
    "tools",
    "research",
    "deep",
    "code",
    "vision",
    "ocr",
    "embedding",
    "rerank",
    "router",
    "extract",
    "train",
    "image",
];
const KNOWN_KV: &[&str] = &["f16", "q8_0", "q4_0", "q4_1", "q5_0", "q5_1", "iq4_nl", "bf16", "f32"];

impl Registry {
    /// Uit de config-snapshot (tabel `models`).
    pub fn from_config(snap: &ConfigSnapshot) -> Result<Self> {
        let v = snap.values.get("models").ok_or_else(|| ModelError::Registry("config/models.toml ontbreekt".into()))?;
        let mut r: Registry = v.clone().try_into().map_err(|e| ModelError::Registry(format!("models.toml: {e}")))?;
        r.config_version = snap.version.clone();
        let errs = r.validate();
        if !errs.is_empty() {
            return Err(ModelError::Registry(errs.join("; ")));
        }
        Ok(r)
    }

    /// Alle problemen tegelijk (lege lijst = geldig).
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        if !self.models.contains_key(&self.default) {
            e.push(format!("default '{}' staat niet in [models]", self.default));
        }
        for (name, s) in &self.servers {
            if !(s.url.starts_with("http://") || s.url.starts_with("https://")) {
                e.push(format!("server '{name}': url moet met http(s):// beginnen"));
            }
            if let Some(m) = &s.model
                && !self.models.contains_key(m)
            {
                e.push(format!("server '{name}': model '{m}' onbekend"));
            }
        }
        for (name, m) in &self.models {
            if m.file.trim().is_empty() {
                e.push(format!("model '{name}': file leeg"));
            }
            if !m.file.to_lowercase().ends_with(".gguf") {
                e.push(format!("model '{name}': alleen .gguf wordt door llama-server geladen"));
            }
            if m.roles.is_empty() {
                e.push(format!("model '{name}': geen rollen"));
            }
            for r in &m.roles {
                if !KNOWN_ROLES.contains(&r.as_str()) {
                    e.push(format!("model '{name}': onbekende rol '{r}'"));
                }
            }
            if !self.servers.contains_key(&m.gpu) {
                e.push(format!("model '{name}': server '{}' bestaat niet", m.gpu));
            }
            if let Some(kv) = &m.kv_type
                && !KNOWN_KV.contains(&kv.as_str())
            {
                e.push(format!("model '{name}': kv_type '{kv}' onbekend"));
            }
            if let Err(err) = parser_by_id(&m.toolcall_parser) {
                e.push(format!("model '{name}': {err}"));
            }
            if m.roles.iter().any(|r| r == "embedding") && m.dims == Some(0) {
                e.push(format!("model '{name}': dims mag niet 0 zijn"));
            }
        }
        e
    }

    /// Model voor een rol (eerste match; default als die de rol heeft).
    pub fn for_role(&self, role: &str) -> Option<(&str, &ModelEntry)> {
        if let Some(m) = self.models.get(&self.default)
            && m.roles.iter().any(|r| r == role)
        {
            return Some((self.default.as_str(), m));
        }
        self.models.iter().find(|(_, m)| m.roles.iter().any(|r| r == role)).map(|(n, m)| (n.as_str(), m))
    }

    pub fn server(&self, name: &str) -> Option<&ServerEntry> {
        self.servers.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str) -> ConfigSnapshot {
        let mut values = toml::Table::new();
        values.insert("models".into(), toml::Value::Table(text.parse().unwrap_or_default()));
        ConfigSnapshot { version: "cfg-t".into(), values, files: Default::default() }
    }

    const GOOD: &str = r#"
default = "a"
[servers.main]
url = "http://127.0.0.1:8080"
[servers.aux]
url = "http://127.0.0.1:8081"
[models.a]
file = "a.gguf"
roles = ["chat", "tools"]
gpu = "main"
kv_type = "q8_0"
[models.e]
file = "e.gguf"
roles = ["embedding"]
gpu = "aux"
dims = 512
"#;

    #[test]
    fn valid_registry_and_role_lookup() -> Result<()> {
        let r = Registry::from_config(&snap(GOOD))?;
        assert_eq!(r.for_role("chat").map(|(n, _)| n), Some("a"));
        assert_eq!(r.for_role("embedding").map(|(n, _)| n), Some("e"));
        assert!(r.for_role("vision").is_none());
        assert_eq!(r.config_version, "cfg-t");
        assert_eq!(r.models["a"].toolcall_parser, "openai-native/v1");
        Ok(())
    }

    #[test]
    fn all_errors_reported_at_once() {
        let bad = r#"
default = "x"
[servers.main]
url = "localhost:8080"
[models.a]
file = "a.bin"
roles = ["vliegen"]
gpu = "gpu9"
kv_type = "q3"
toolcall_parser = "nope"
"#;
        let e = Registry::from_config(&snap(bad)).err().map(|e| e.to_string()).unwrap_or_default();
        for frag in ["default 'x'", "http(s)", ".gguf", "vliegen", "gpu9", "q3", "nope"] {
            assert!(e.contains(frag), "mist '{frag}' in: {e}");
        }
    }

    #[test]
    fn repo_config_is_valid() -> Result<()> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/models.toml");
        let text = std::fs::read_to_string(dir).map_err(|e| ModelError::Registry(e.to_string()))?;
        Registry::from_config(&snap(&text))?;
        Ok(())
    }
}
