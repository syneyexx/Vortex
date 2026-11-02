//! Secrets broker: API-sleutels komen uit de OS-keystore (Windows Credential Manager,
//! macOS Keychain, Linux keyutils) en nooit in context, geheugen of logs. Techniek, geen rem.
// VX:W01.14

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{CoreError, Result};

/// Geheime waarde. `Debug`/`Display` tonen nooit de inhoud.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Arc<str>);

impl Secret {
    pub fn new(v: impl Into<String>) -> Self {
        Self(Arc::from(v.into()))
    }
    /// Expliciet de waarde gebruiken (bv. in een HTTP-header). Nooit loggen.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}
impl serde::Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str("***")
    }
}

/// Opslag voor secrets.
pub trait SecretBackend: Send + Sync + fmt::Debug {
    fn get(&self, name: &str) -> Result<Option<Secret>>;
    fn set(&self, name: &str, value: &str) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
}

/// OS-keystore via `keyring`.
#[derive(Debug, Clone)]
pub struct KeyringBackend {
    service: String,
}

impl KeyringBackend {
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }
    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, name).map_err(|e| CoreError::Secrets(e.to_string()))
    }
}

impl SecretBackend for KeyringBackend {
    fn get(&self, name: &str) -> Result<Option<Secret>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(Secret::new(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Secrets(e.to_string())),
        }
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.entry(name)?.set_password(value).map_err(|e| CoreError::Secrets(e.to_string()))
    }
    fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Secrets(e.to_string())),
        }
    }
}

/// In-memory (tests, of als de keystore niet beschikbaar is).
#[derive(Debug, Default)]
pub struct MemoryBackend(RwLock<HashMap<String, String>>);

impl SecretBackend for MemoryBackend {
    fn get(&self, name: &str) -> Result<Option<Secret>> {
        Ok(self.0.read().get(name).cloned().map(Secret::new))
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.0.write().insert(name.into(), value.into());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<()> {
        self.0.write().remove(name);
        Ok(())
    }
}

/// De broker: één toegangspunt; onthoudt welke namen bekend zijn (niet de waarden in logs).
#[derive(Debug, Clone)]
pub struct SecretBroker {
    backend: Arc<dyn SecretBackend>,
}

impl SecretBroker {
    pub fn new(backend: Arc<dyn SecretBackend>) -> Self {
        Self { backend }
    }
    /// Standaard: OS-keystore onder service "vortex".
    pub fn os() -> Self {
        Self::new(Arc::new(KeyringBackend::new("vortex")))
    }
    pub fn memory() -> Self {
        Self::new(Arc::new(MemoryBackend::default()))
    }
    pub fn get(&self, name: &str) -> Result<Option<Secret>> {
        self.backend.get(name)
    }
    pub fn require(&self, name: &str) -> Result<Secret> {
        self.get(name)?.ok_or_else(|| CoreError::Secrets(format!("secret '{name}' ontbreekt")))
    }
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        self.backend.set(name, value)
    }
    pub fn delete(&self, name: &str) -> Result<()> {
        self.backend.delete(name)
    }

    /// Vervang bekende secret-waarden in tekst door `***` (vangnet vóór loggen/opslaan).
    pub fn redact(&self, text: &str, names: &[&str]) -> String {
        let mut out = text.to_string();
        for n in names {
            if let Ok(Some(s)) = self.get(n)
                && !s.expose().is_empty()
            {
                out = out.replace(s.expose(), "***");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_never_prints_value() {
        let s = Secret::new("sk-geheim");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(format!("{s}"), "***");
        assert_eq!(serde_json::to_string(&s).unwrap_or_default(), "\"***\"");
        assert_eq!(s.expose(), "sk-geheim");
    }

    #[test]
    fn broker_get_set_delete_and_redact() -> Result<()> {
        let b = SecretBroker::memory();
        assert!(b.get("x")?.is_none());
        assert!(b.require("x").is_err());
        b.set("x", "abc123")?;
        assert_eq!(b.require("x")?.expose(), "abc123");
        assert_eq!(b.redact("token=abc123;", &["x"]), "token=***;");
        b.delete("x")?;
        assert!(b.get("x")?.is_none());
        Ok(())
    }
}
