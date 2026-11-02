//! Rollen met prompts in `config/roles/<rol>.md` (frontmatter: temperature, max_tokens,
//! model_role). Ontbrekende rollen krijgen de ingebouwde standaardprompt; [`ensure_defaults`]
//! schrijft ze naar schijf zodat ze in Instellingen te bewerken zijn.
// VX:W09.01

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vortex_core::ConfigSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub prompt: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Welk model (rol in models.toml): "chat" (hoofdmodel) of "extract" (1060).
    pub model_role: String,
}

pub const ROLE_NAMES: &[&str] = &[
    "planner",
    "solver",
    "factchecker",
    "critic",
    "specialist-code",
    "judge",
    "synthesizer",
    "conflict-researcher",
    "source-assessor",
    "conflict-judge",
];

pub fn default_prompt(name: &str) -> (&'static str, f32, &'static str) {
    match name {
        "planner" => (
            "Je bent de Planner in het Vortex-team. Splits de vraag op in 2 tot 5 concrete deelvragen of stappen die samen tot een goed antwoord leiden. Geen antwoord geven, alleen de stappen. Antwoord als JSON: {\"steps\": [\"…\"]}.",
            0.2,
            "chat",
        ),
        "solver" => (
            "Je bent een Solver in het Vortex-team. Werk de vraag zelfstandig en volledig uit vanuit de gegeven invalshoek. Wees concreet, reken na waar het kan, noem aannames expliciet en zeg wat je niet zeker weet. Schrijf in het Nederlands.",
            0.7,
            "chat",
        ),
        "factchecker" => (
            "Je bent de Fact-checker. Splits de tekst in losse, controleerbare feitelijke beweringen (maximaal 6, alleen echte feiten — geen meningen of stappen). Antwoord als JSON: {\"claims\": [\"…\"]}.",
            0.0,
            "extract",
        ),
        "critic" => (
            "Je bent de Critic (advocaat van de duivel). Zoek fouten, gaten, onjuiste aannames, rekenfouten en tegenvoorbeelden in het antwoord. Wees streng maar eerlijk; verzin geen problemen. Antwoord als JSON: {\"issues\": [{\"severity\": \"high|medium|low\", \"text\": \"…\"}]}.",
            0.2,
            "chat",
        ),
        "specialist-code" => (
            "Je bent de code-reviewer. Beoordeel de code op correctheid, randgevallen en leesbaarheid. Testresultaten (als ze er zijn) wegen zwaarder dan je eigen indruk. Antwoord als JSON: {\"issues\": [{\"severity\": \"high|medium|low\", \"text\": \"…\"}]}.",
            0.1,
            "chat",
        ),
        "judge" => (
            "Je bent de Judge. Weeg het bewijs: objectieve checks (tests, berekeningen, bronnen) wegen zwaarder dan overeenstemming tussen solvers — overeenstemming bewijst niets. Keur alleen goed als je het antwoord vertrouwt. Antwoord als JSON: {\"verdict\": \"accept|revise|uncertain\", \"confidence\": 0.0-1.0, \"reasons\": [\"…\"], \"fix\": \"wat er beter moet (bij revise)\"}.",
            0.0,
            "chat",
        ),
        "synthesizer" => (
            "Je bent de Synthesizer. Schrijf het eindantwoord voor de gebruiker op basis van het goedgekeurde concept van het team, in de stijl en toon van de gebruiker. Voeg geen nieuwe feiten toe; laat weg wat de Judge afkeurde; wees eerlijk over onzekerheid.",
            0.4,
            "chat",
        ),
        "conflict-researcher" => (
            "Je bent onderzoeker in het conflict-team. Je verdedigt één kant van een tegenstrijdigheid: geef het sterkste eerlijke pleidooi met bewijs (bron, datum, type bron). Verzin geen bronnen. Antwoord als JSON: {\"argument\": \"…\", \"evidence\": [\"…\"], \"time_bound\": true|false}.",
            0.3,
            "chat",
        ),
        "source-assessor" => (
            "Je bent bronbeoordelaar. Beoordeel per kant: primair of secundair, actualiteit, onafhankelijkheid (kopieën van dezelfde bron tellen als één). Antwoord als JSON: {\"a_quality\": 0.0-1.0, \"b_quality\": 0.0-1.0, \"notes\": \"…\"}.",
            0.0,
            "chat",
        ),
        "conflict-judge" => (
            "Je bent de Judge van het conflict-team. Kies: A wint, B wint, beide waar in een eigen periode (tijdgebonden), of betwist (niet te beslissen). Eén webpagina wint niet automatisch. Bij persoonlijke feiten wint wat de gebruiker zelf zei. Antwoord als JSON: {\"decision\": \"a|b|periods|disputed\", \"reason\": \"…\", \"switch_date\": \"JJJJ-MM-DD of leeg\"}.",
            0.0,
            "chat",
        ),
        _ => ("", 0.5, "chat"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Roles {
    map: BTreeMap<String, Role>,
}

fn parse_file(name: &str, text: &str) -> Role {
    let (dp, dt, dm) = default_prompt(name);
    let (fm, body) = match text.strip_prefix("---") {
        Some(rest) => match rest.find("\n---") {
            Some(e) => (rest[..e].to_string(), rest[e + 4..].trim_start_matches(['\r', '\n']).to_string()),
            None => (String::new(), text.to_string()),
        },
        None => (String::new(), text.to_string()),
    };
    let get = |k: &str| {
        fm.lines().find_map(|l| l.split_once(':').filter(|(a, _)| a.trim() == k).map(|(_, v)| v.trim().to_string()))
    };
    Role {
        name: name.to_string(),
        prompt: if body.trim().is_empty() { dp.to_string() } else { body.trim().to_string() },
        temperature: get("temperature").and_then(|v| v.parse().ok()).or(Some(dt)),
        max_tokens: get("max_tokens").and_then(|v| v.parse().ok()),
        model_role: get("model_role").unwrap_or_else(|| dm.to_string()),
    }
}

impl Roles {
    /// Uit de config-snapshot (`roles/<naam>.md`), aangevuld met standaarden.
    pub fn from_snapshot(snap: &ConfigSnapshot) -> Self {
        let mut map = BTreeMap::new();
        for n in ROLE_NAMES {
            let r = match snap.file(&format!("roles/{n}.md")) {
                Some(t) => parse_file(n, t),
                None => parse_file(n, ""),
            };
            map.insert(n.to_string(), r);
        }
        Self { map }
    }

    pub fn defaults() -> Self {
        Self::from_snapshot(&ConfigSnapshot::empty())
    }

    pub fn get(&self, name: &str) -> Role {
        self.map.get(name).cloned().unwrap_or_else(|| parse_file(name, ""))
    }
}

/// Schrijf ontbrekende rolbestanden (bewerkbaar in Instellingen).
pub fn ensure_defaults(config_dir: &Path) -> std::io::Result<usize> {
    let dir = config_dir.join("roles");
    std::fs::create_dir_all(&dir)?;
    let mut n = 0;
    for name in ROLE_NAMES {
        let p = dir.join(format!("{name}.md"));
        if !p.exists() {
            let (prompt, t, m) = default_prompt(name);
            std::fs::write(&p, format!("---\ntemperature: {t}\nmodel_role: {m}\n---\n{prompt}\n"))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_overrides_default() {
        let r = parse_file("judge", "---\ntemperature: 0.3\nmodel_role: extract\n---\nEigen judge.\n");
        assert_eq!(r.prompt, "Eigen judge.");
        assert_eq!(r.temperature, Some(0.3));
        assert_eq!(r.model_role, "extract");
        let d = Roles::defaults().get("critic");
        assert!(d.prompt.contains("advocaat van de duivel"));
    }
}
