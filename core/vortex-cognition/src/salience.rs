//! Salience/valentie-tags (W07.07): belangrijk, urgent, nieuw, tegenstrijdig, emotie.
// VX:W07.07

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Salience {
    pub important: f64,
    pub urgent: bool,
    /// 1 = helemaal nieuw voor het geheugen.
    pub novelty: f64,
    pub contradictory: bool,
    /// -1 negatief … +1 positief.
    pub valence: f64,
}

const POS: &[&str] = &["top", "super", "goed", "fijn", "dank", "geweldig", "mooi", "blij", "great", "thanks"];
const NEG: &[&str] =
    &["slecht", "fout", "kapot", "boos", "frustr", "irritant", "jammer", "werkt niet", "probleem", "bug", "hate"];

/// `best_memory` = hoogste geheugenscore (nieuw als laag); `conflicts` = open conflicten.
pub fn tag(text: &str, best_memory: Option<f64>, conflicts: usize) -> Salience {
    let l = text.to_lowercase();
    let pos = POS.iter().filter(|w| l.contains(*w)).count() as f64;
    let neg = NEG.iter().filter(|w| l.contains(*w)).count() as f64;
    let urgent =
        ["dringend", "urgent", "nu meteen", "asap", "deadline", "vandaag nog", "snel"].iter().any(|w| l.contains(w))
            || l.matches('!').count() >= 2;
    let important = (vortex_memory::policy::importance(text, vortex_memory::MemoryKind::Episodic).0
        + if urgent { 0.2 } else { 0.0 })
    .min(1.0);
    Salience {
        important,
        urgent,
        novelty: 1.0 - best_memory.unwrap_or(0.0).clamp(0.0, 1.0),
        contradictory: conflicts > 0,
        valence: if pos + neg == 0.0 { 0.0 } else { (pos - neg) / (pos + neg) },
    }
}

/// Featurevector voor het salience-net (W11).
pub fn vector(s: &Salience, words: usize) -> Vec<f32> {
    vec![
        s.important as f32,
        if s.urgent { 1.0 } else { 0.0 },
        s.novelty as f32,
        if s.contradictory { 1.0 } else { 0.0 },
        s.valence as f32,
        (words as f32 / 100.0).min(5.0),
    ]
}

/// Salience met net (W11): het net schat "belangrijk voor de gebruiker" (0..1).
pub fn tag_with(
    text: &str,
    best_memory: Option<f64>,
    conflicts: usize,
    scorer: &vortex_core::Scorer,
    reference: &str,
) -> Salience {
    let mut s = tag(text, best_memory, conflicts);
    let out = vortex_core::neural::decide(
        scorer,
        "salience",
        &vector(&s, text.split_whitespace().count()),
        vec![s.important as f32],
        reference,
    );
    if let Some(v) = out.first() {
        s.important = f64::from(*v).clamp(0.0, 1.0);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags() {
        let s = tag("Dringend: de build is kapot!!", Some(0.9), 0);
        assert!(s.urgent && s.valence < 0.0 && s.novelty < 0.2);
        assert!(tag("top, dank je", None, 1).contradictory);
    }
}
