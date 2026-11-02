//! Toolfouten, ingedeeld zodat herstel weet wat te doen (W08.11).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorKind {
    /// Onbekende tool.
    NotFound,
    /// Invoer klopt niet met het schema → model moet invoer aanpassen.
    InvalidInput,
    /// Preconditie niet voldaan (bv. bestand bestaat niet).
    Precondition,
    /// Postconditie faalt: tool "lukte" maar het doel is niet bereikt.
    Postcondition,
    /// Buiten de capability handle van deze run.
    Capability,
    Timeout,
    Cancelled,
    /// Tijdelijk (netwerk, bezet) → opnieuw proberen mag.
    Transient,
    /// Tool faalde inhoudelijk (bv. script gaf exitcode 1).
    Failed,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, Serialize, Deserialize)]
#[error("{kind:?}: {message}")]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub message: String,
    /// Concrete hint voor het model (wat nu te doen).
    #[serde(default)]
    pub hint: Option<String>,
}

impl ToolError {
    pub fn new(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), hint: None }
    }
    pub fn with_hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn invalid(m: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::InvalidInput, m)
    }
    pub fn failed(m: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::Failed, m)
    }
    pub fn transient(m: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::Transient, m)
    }
    pub fn retryable(&self) -> bool {
        matches!(self.kind, ToolErrorKind::Transient | ToolErrorKind::Timeout)
    }
    /// Tekst voor het model: fout + wat te doen.
    pub fn for_model(&self) -> String {
        let what = match self.kind {
            ToolErrorKind::NotFound => "Deze tool bestaat niet. Kies een tool uit de lijst.",
            ToolErrorKind::InvalidInput => "Pas de invoer aan volgens het schema en probeer opnieuw.",
            ToolErrorKind::Precondition => {
                "Voorwaarde niet voldaan; controleer eerst (bv. met list_dir/read_file) of het doel bestaat."
            }
            ToolErrorKind::Postcondition => {
                "De actie liep, maar het doel is niet bereikt. Controleer het resultaat en kies een andere aanpak."
            }
            ToolErrorKind::Capability => {
                "Buiten het toegestane bereik van deze taak. Kies een pad of bron binnen het bereik."
            }
            ToolErrorKind::Timeout => "Duurde te lang. Maak de opdracht kleiner of kies een andere route.",
            ToolErrorKind::Cancelled => "Geannuleerd.",
            ToolErrorKind::Transient => "Tijdelijke fout; later opnieuw of andere bron.",
            ToolErrorKind::Failed => "De tool meldde een fout. Lees de melding en pas je aanpak aan.",
        };
        match &self.hint {
            Some(h) => format!("FOUT ({:?}): {}\n{what}\nHint: {h}", self.kind, self.message),
            None => format!("FOUT ({:?}): {}\n{what}", self.kind, self.message),
        }
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        let kind = match e.kind() {
            std::io::ErrorKind::NotFound => ToolErrorKind::Precondition,
            std::io::ErrorKind::TimedOut => ToolErrorKind::Timeout,
            std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock => ToolErrorKind::Transient,
            _ => ToolErrorKind::Failed,
        };
        Self::new(kind, e.to_string())
    }
}
