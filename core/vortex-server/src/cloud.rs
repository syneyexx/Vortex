//! Optionele cloud-escalatie (W17.17): standaard UIT. Alleen als de gebruiker het aanzet
//! (`config/cloud.toml`, sleutel in de secret-opslag) gaat een vraag naar een externe
//! OpenAI-compatibele dienst — expliciet (model "cloud") of automatisch bij moeilijke vragen in
//! een zwak taaktype. Elke escalatie wordt gelogd (wat, waarom, hoeveel tekens) en is begrensd per dag.
// VX:W17.17

use std::sync::Arc;

use serde::Deserialize;
use vortex_models::OpenAiBackend;
use vortex_types::{Event, EventKind, TraceId};

use crate::chat::{ChatEngine, ChatError};

#[derive(Debug, Clone, Deserialize)]
pub struct CloudCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "key")]
    pub key_secret: String,
    /// Automatisch escaleren (moeilijk + zwak taaktype). Expliciet kiezen kan altijd als `enabled`.
    #[serde(default)]
    pub auto: bool,
    #[serde(default = "twenty")]
    pub max_per_day: usize,
}
fn key() -> String {
    "cloud_api_key".into()
}
fn twenty() -> usize {
    20
}
impl Default for CloudCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model: String::new(),
            key_secret: key(),
            auto: false,
            max_per_day: 20,
        }
    }
}

impl ChatEngine {
    pub fn cloud_cfg(&self) -> CloudCfg {
        self.kernel.config().current().values.get("cloud").and_then(|v| v.clone().try_into().ok()).unwrap_or_default()
    }

    fn cloud_used_today(&self) -> usize {
        let day = chrono::Utc::now().date_naive().to_string();
        self.kernel
            .events()
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM events WHERE kind = 'chat.cloud.escalated' AND ts >= ?1",
                    [&day],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap_or(0) as usize
    }

    /// Backend voor de clouddienst, als dat mag.
    pub(crate) fn cloud_backend(&self) -> Result<Arc<OpenAiBackend>, ChatError> {
        let c = self.cloud_cfg();
        if !c.enabled || c.base_url.is_empty() || c.model.is_empty() {
            return Err(ChatError::Bad("cloud-escalatie staat uit (config/cloud.toml)".into()));
        }
        if self.cloud_used_today() >= c.max_per_day {
            return Err(ChatError::Bad(format!("daglimiet cloud-escalatie bereikt ({})", c.max_per_day)));
        }
        let key = self
            .kernel
            .secrets()
            .get(&c.key_secret)
            .ok()
            .flatten()
            .map(|s| s.expose().to_string())
            .ok_or_else(|| ChatError::Bad(format!("geen sleutel '{}' in de secret-opslag", c.key_secret)))?;
        let caps = vortex_models::backend::Capabilities {
            chat: true,
            streaming: true,
            tools: true,
            json_schema: true,
            ..Default::default()
        };
        let b = OpenAiBackend::new("cloud", &c.base_url, &c.model, &caps, "openai-native/v1")
            .map_err(ChatError::Model)?
            .with_api_key(key);
        Ok(Arc::new(b))
    }

    /// Automatisch escaleren? Alleen als aan, en de vraag moeilijk is in een zwak taaktype.
    pub(crate) fn cloud_auto(&self, text: &str) -> Option<String> {
        let c = self.cloud_cfg();
        if !(c.enabled && c.auto) {
            return None;
        }
        let r = vortex_cognition::route(text);
        let score = self.self_model.read().score(r.task);
        (r.difficulty >= 0.8 && score.is_some_and(|s| s < 0.5)).then(|| {
            format!(
                "moeilijk ({:.2}) en zwak taaktype {:?} ({:.0}%)",
                r.difficulty,
                r.task,
                score.unwrap_or(0.0) * 100.0
            )
        })
    }

    pub(crate) fn log_cloud(&self, reason: &str, chars: usize) {
        let c = self.cloud_cfg();
        let _ = self.kernel.events().append(Event::new(
            EventKind::CloudEscalated,
            TraceId::new(),
            serde_json::json!({"reason": reason, "model": c.model, "base_url": c.base_url, "chars": chars}),
        ));
    }
}
