//! Haak voor framework-eigen neurale layers (W11): consumenten (router, geheugenpolicy,
//! retrieval, skills, …) vragen een beslissing met hun eigen featurevector en heuristische
//! uitkomst; de implementatie (`vortex-neural`) kiest heuristiek, schaduw of het actieve net.
//! Zo kan elke crate een layer gebruiken zonder van `vortex-neural` af te hangen.
// VX:W11.04

use std::sync::Arc;

pub trait LayerScorer: Send + Sync + std::fmt::Debug {
    /// Beslissing voor `layer`. Geen actieve versie → `heuristic` terug. In schaduwmodus draait
    /// het net mee (gelogd), maar telt de heuristiek. `reference` (run-ID, herinnering-ID, …)
    /// koppelt het voorbeeld later aan zijn uitkomst (feature logging, W11.01).
    fn decide(&self, layer: &str, features: &[f32], heuristic: Vec<f32>, reference: &str) -> Vec<f32>;
}

/// Optionele scorer (geen = altijd heuristiek).
pub type Scorer = Option<Arc<dyn LayerScorer>>;

/// Helper: beslissing of heuristiek.
pub fn decide(s: &Scorer, layer: &str, features: &[f32], heuristic: Vec<f32>, reference: &str) -> Vec<f32> {
    match s {
        Some(s) => s.decide(layer, features, heuristic, reference),
        None => heuristic,
    }
}

/// Index van de grootste waarde.
pub fn argmax(v: &[f32]) -> usize {
    v.iter().enumerate().fold((0, f32::MIN), |(bi, bv), (i, x)| if *x > bv { (i, *x) } else { (bi, bv) }).0
}
