//! Bewijs per bewering + bewijsoverzicht per conclusie (W09.08).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    Supported,
    Contradicted,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// "geheugen" | "web" | "test" | "berekening"
    pub kind: String,
    pub reference: String,
    #[serde(default)]
    pub date: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimCheck {
    pub claim: String,
    pub label: Label,
    pub why: String,
    pub sources: Vec<Source>,
    /// Waar geverifieerd: "lokaal" of "lokaal+web".
    pub scope: String,
}

impl ClaimCheck {
    pub fn score(checks: &[ClaimCheck]) -> f64 {
        if checks.is_empty() {
            return 0.0;
        }
        let s: f64 = checks
            .iter()
            .map(|c| match c.label {
                Label::Supported => 1.0,
                Label::Contradicted => -2.0,
                Label::Unverified => 0.0,
            })
            .sum();
        s / checks.len() as f64
    }
}

/// Tekstblok voor Judge/Synthesizer.
pub fn summary(checks: &[ClaimCheck]) -> String {
    checks
        .iter()
        .map(|c| {
            let l = match c.label {
                Label::Supported => "GESTAAFD",
                Label::Contradicted => "TEGENGESPROKEN",
                Label::Unverified => "onbevestigd",
            };
            let src = c.sources.first().map(|s| format!(" [{}: {}]", s.kind, s.reference)).unwrap_or_default();
            format!("- {l}: {} — {}{src}", c.claim, c.why)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
