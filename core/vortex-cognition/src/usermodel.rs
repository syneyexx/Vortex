//! Gebruikersmodel (W07.12): voorkeuren, stijl en toon, lange-termijndoelen, projectvoorkeuren —
//! afgeleid uit het geheugen (user/project) en feedback. Wordt in de prompt gezet (stijl- en
//! toonaanpassing) en stuurt gepersonaliseerd ophalen.
// VX:W07.12

use serde::{Deserialize, Serialize};
use vortex_memory::{Filters, MemoryKind, MemoryStore};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    pub language: String,
    pub style: Vec<String>,
    pub facts: Vec<String>,
    pub goals: Vec<String>,
    pub project: Vec<String>,
    pub feedback: FeedbackStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackStats {
    pub good: i64,
    pub bad: i64,
    pub corrections: i64,
}

const STYLE_HINTS: &[(&str, &str)] = &[
    ("kort", "Antwoord kort en to-the-point."),
    ("bondig", "Antwoord kort en to-the-point."),
    ("uitgebreid", "Geef uitgebreide uitleg."),
    ("stap voor stap", "Leg stap voor stap uit."),
    ("geen emoji", "Gebruik geen emoji."),
    ("informeel", "Schrijf informeel (je/jij)."),
    ("formeel", "Schrijf formeel."),
    ("engels", "De gebruiker wil Engels."),
    ("in het nederlands", "Antwoord in het Nederlands."),
    ("code eerst", "Begin met de code, daarna uitleg."),
];

impl UserProfile {
    /// Uit geheugen + feedbacktabel.
    pub fn build(store: &MemoryStore, workspace: Option<&str>) -> Self {
        let mut p = UserProfile { language: "nl".into(), ..Default::default() };
        let users = store
            .list(&Filters { kinds: vec![MemoryKind::User], ..Default::default() }, None, 200, 0)
            .unwrap_or_default();
        let mut by_imp = users.clone();
        by_imp.sort_by(|a, b| b.importance.total_cmp(&a.importance));
        for m in by_imp.iter().take(12) {
            let l = m.content.to_lowercase();
            for (k, hint) in STYLE_HINTS {
                if l.contains(k)
                    && (l.contains("antwoord")
                        || l.contains("wil")
                        || l.contains("graag")
                        || l.contains("liever")
                        || l.contains("voorkeur"))
                    && !p.style.contains(&hint.to_string())
                {
                    p.style.push(hint.to_string());
                }
            }
            if ["doel", "ik wil", "plan om", "wil graag", "mijn ambitie"].iter().any(|k| l.contains(k)) {
                p.goals.push(m.content.chars().take(160).collect());
            } else {
                p.facts.push(m.content.chars().take(160).collect());
            }
        }
        if p.style.iter().any(|s| s.contains("Engels")) {
            p.language = "en".into();
        }
        if let Some(ws) = workspace {
            let proj = store
                .list(
                    &Filters { kinds: vec![MemoryKind::Project], workspace: Some(ws.into()), ..Default::default() },
                    None,
                    20,
                    0,
                )
                .unwrap_or_default();
            p.project = proj
                .into_iter()
                .filter(|m| m.workspace.as_deref() == Some(ws))
                .map(|m| m.content.chars().take(160).collect())
                .collect();
        }
        p.feedback = store
            .log
            .db()
            .read(|c| {
                let q = |r: &str| -> rusqlite::Result<i64> {
                    c.query_row("SELECT COUNT(*) FROM feedback WHERE rating = ?1", [r], |x| x.get(0))
                };
                Ok(FeedbackStats { good: q("good")?, bad: q("bad")?, corrections: q("correction")? })
            })
            .unwrap_or_default();
        if p.feedback.bad + p.feedback.corrections > p.feedback.good + 3 {
            p.style.push("Recente antwoorden werden vaak afgekeurd: wees voorzichtiger, controleer feiten en zeg wanneer je iets niet zeker weet.".into());
        }
        p
    }

    /// Tekstblok voor de prompt (leeg als er niets bekend is).
    pub fn prompt_block(&self) -> Option<String> {
        if self.style.is_empty() && self.facts.is_empty() && self.goals.is_empty() && self.project.is_empty() {
            return None;
        }
        let mut s = String::new();
        if !self.style.is_empty() {
            s.push_str("Stijl: ");
            s.push_str(&self.style.join(" "));
            s.push('\n');
        }
        for f in self.facts.iter().take(6) {
            s.push_str(&format!("- {f}\n"));
        }
        for g in self.goals.iter().take(4) {
            s.push_str(&format!("- Doel: {g}\n"));
        }
        for p in self.project.iter().take(4) {
            s.push_str(&format!("- Project: {p}\n"));
        }
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vortex_core::{Db, EventLog, db::CORE_MIGRATIONS};
    use vortex_memory::{HashEmbedder, HeuristicLlm, MemConfig, NewMemory};
    use vortex_types::{Provenance, SourceKind};

    #[tokio::test]
    async fn profile_from_memory() {
        let store = MemoryStore::open(
            EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap_or_else(|e| panic!("{e}")))),
            Arc::new(HashEmbedder::new(64)),
            Arc::new(HeuristicLlm),
            MemConfig { dims: 64, ..Default::default() },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for t in [
            "Ik wil graag korte antwoorden, liever kort dan lang.",
            "Mijn doel is om Vortex dit jaar af te maken.",
            "Ik werk met een RTX 5060 Ti.",
        ] {
            let mut m = NewMemory::new(MemoryKind::User, t, Provenance::new(SourceKind::User));
            m.force = true;
            store.write(m).await.unwrap_or_else(|e| panic!("{e}"));
        }
        let p = UserProfile::build(&store, None);
        assert!(p.style.iter().any(|s| s.contains("kort")), "{p:?}");
        assert!(p.goals.iter().any(|g| g.contains("Vortex")));
        assert!(p.facts.iter().any(|f| f.contains("5060")));
        assert!(p.prompt_block().unwrap_or_default().contains("Stijl:"));
    }
}
