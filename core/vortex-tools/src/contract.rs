//! Toolcontract (W08.01): schema's, bijwerkingen, timeout, annuleerbaar, herhaalbaar, resources.

use serde::{Deserialize, Serialize};

/// Bijwerking van een tool. Bepaalt parallel/sequentieel, idempotentie en preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Alleen lezen (lokaal of web): vrij herhaalbaar en parallel.
    Read,
    /// Verandert lokale toestand (bestanden, database).
    Local,
    /// Verandert iets buiten de machine (API-call met effect).
    External,
}

impl Effect {
    pub fn as_core(self) -> vortex_core::EffectKind {
        match self {
            Effect::Read => vortex_core::EffectKind::Read,
            Effect::Local => vortex_core::EffectKind::Local,
            Effect::External => vortex_core::EffectKind::External,
        }
    }
}

/// Welke middelen een tool gebruikt (voor de Dirigent en effectanalyse).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum ResourceUse {
    Cpu,
    Gpu,
    Net,
    Disk,
    /// Benoemde resource (bv. een databasebestand) — twee schrijvers erop = sequentieel.
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolContract {
    pub name: String,
    /// Eén zin: wat de tool doet (dit ziet het model bij discovery).
    pub description: String,
    /// JSON-schema van de invoer.
    pub input_schema: serde_json::Value,
    /// JSON-schema van de uitvoer (optioneel; wordt gecontroleerd als het er is).
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    pub effect: Effect,
    pub timeout_ms: u64,
    pub cancellable: bool,
    /// Zelfde invoer opnieuw uitvoeren is veilig (zonder ledger).
    pub idempotent: bool,
    #[serde(default)]
    pub resources: Vec<ResourceUse>,
    /// Netwerkbestemmingen (hosts) — documentatie + effectanalyse.
    #[serde(default)]
    pub net_destinations: Vec<String>,
    /// Resultaat mag gecachet worden (alleen zinvol bij Read).
    #[serde(default)]
    pub cacheable: bool,
    /// Uit welke bron: "builtin", "mcp:<server>", "skill:<naam>", "server".
    #[serde(default = "builtin")]
    pub origin: String,
    #[serde(default = "v1")]
    pub version: String,
}

fn builtin() -> String {
    "builtin".into()
}
fn v1() -> String {
    "1".into()
}

impl ToolContract {
    pub fn new(name: &str, description: &str, effect: Effect, input_schema: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            output_schema: None,
            effect,
            timeout_ms: 30_000,
            cancellable: true,
            idempotent: effect == Effect::Read,
            resources: vec![],
            net_destinations: vec![],
            cacheable: false,
            origin: builtin(),
            version: v1(),
        }
    }
    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
    pub fn resources(mut self, r: Vec<ResourceUse>) -> Self {
        self.resources = r;
        self
    }
    pub fn cacheable(mut self) -> Self {
        self.cacheable = true;
        self
    }
    pub fn origin(mut self, o: &str) -> Self {
        self.origin = o.into();
        self
    }
    pub fn output(mut self, s: serde_json::Value) -> Self {
        self.output_schema = Some(s);
        self
    }
    pub fn net(mut self, hosts: Vec<String>) -> Self {
        self.net_destinations = hosts;
        self
    }
}
