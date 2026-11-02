//! Router (W07.06): taaktype + moeilijkheid uit de vraag. Heuristiek nu; het router-net volgt in
//! W11 en leert van de gelogde features (DecisionRecord) + LM-labels.
// VX:W07.06

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Chat,
    Code,
    Math,
    Analysis,
    Research,
    Planning,
    Personal,
    Factual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteFeatures {
    pub chars: usize,
    pub words: usize,
    pub questions: usize,
    pub has_code: bool,
    pub has_numbers: bool,
    pub math_ops: usize,
    pub analysis_words: usize,
    pub research_words: usize,
    pub planning_words: usize,
    pub personal: bool,
    pub time_sensitive: bool,
    pub small_talk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub task: TaskType,
    /// 0 = triviaal, 1 = zeer moeilijk.
    pub difficulty: f64,
    pub features: RouteFeatures,
    pub confidence: f64,
}

fn count(l: &str, words: &[&str]) -> usize {
    words.iter().filter(|w| l.contains(*w)).count()
}

pub fn features(text: &str) -> RouteFeatures {
    let l = text.to_lowercase();
    let words = l.split_whitespace().count();
    RouteFeatures {
        chars: text.len(),
        words,
        questions: text.matches('?').count(),
        // Sterke signalen; een taalnaam ("Rust", "SQLite") noemen is nog geen codeertaak.
        has_code: text.contains("```")
            || count(
                &l,
                &[
                    "fn ",
                    "def ",
                    "class ",
                    "function(",
                    "compile",
                    "compileer",
                    "error[",
                    "traceback",
                    "stack trace",
                    "regex",
                    "bug",
                    "code",
                    "script",
                    "schrijf een functie",
                    "functie die",
                    "select ",
                    "unwrap()",
                ],
            ) >= 1,
        has_numbers: l.chars().any(|c| c.is_ascii_digit()),
        math_ops: l.matches(['+', '×', '*', '/', '=', '%', '^']).count()
            + count(&l, &["bereken", "hoeveel", "som van", "kans", "procent", "gemiddelde", "wortel"]),
        analysis_words: count(
            &l,
            &[
                "waarom",
                "analyseer",
                "vergelijk",
                "verschil",
                "voor- en nadelen",
                "afweging",
                "evalueer",
                "verklaar",
                "beoordeel",
                "bewijs",
                "redeneer",
            ],
        ),
        research_words: count(
            &l,
            &[
                "zoek uit",
                "onderzoek",
                "bronnen",
                "laatste nieuws",
                "recent",
                "studie",
                "paper",
                "wat zeggen experts",
                "literatuur",
            ],
        ),
        planning_words: count(&l, &["plan", "stappen", "stappenplan", "roadmap", "strategie", "hoe pak ik", "schema"]),
        personal: count(&l, &[" mijn ", "ik ", " me ", "mij ", "mijn "]) > 0
            || l.starts_with("mijn")
            || l.starts_with("ik"),
        time_sensitive: count(
            &l,
            &["vandaag", "nu ", "huidige", "laatste", "2025", "2026", "deze week", "koers", "prijs"],
        ) > 0,
        small_talk: words <= 5
            && count(&l, &["hoi", "hallo", "hey", "dank", "bedankt", "goedemorgen", "hoe gaat", "ok", "top"]) > 0,
    }
}

fn base_difficulty(task: TaskType) -> f64 {
    match task {
        TaskType::Chat => 0.05,
        TaskType::Personal | TaskType::Factual => 0.25,
        TaskType::Math => 0.45,
        TaskType::Code => 0.5,
        TaskType::Planning => 0.5,
        TaskType::Analysis => 0.6,
        TaskType::Research => 0.7,
    }
}

/// Onder deze zekerheid vraagt de router een label aan het kleine routermodel.
pub const LM_LABEL_BELOW: f64 = 0.65;

/// Twijfelachtige heuristische route laten labelen door een klein LM (JSON-schema, 1 token-achtig
/// antwoord). Bij fouten of onzin blijft de heuristiek staan. VX:W07.06
pub async fn refine_with_lm(llm: &dyn crate::llm::CogLlm, text: &str, r: Route) -> Route {
    if r.confidence >= LM_LABEL_BELOW {
        return r;
    }
    const TASKS: [&str; 8] = ["chat", "code", "math", "analysis", "research", "planning", "personal", "factual"];
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"task": {"type": "string", "enum": TASKS}},
        "required": ["task"]
    });
    let msgs = vec![
        vortex_models::backend::Message::system(
            "Classificeer het verzoek van de gebruiker in precies één taaktype: chat (praatje), code, math, analysis (vergelijken/afwegen), research (bronnen zoeken), planning, personal (over de gebruiker zelf), factual (feitvraag). Antwoord als JSON.",
        ),
        vortex_models::backend::Message::user(text.chars().take(1500).collect::<String>()),
    ];
    let opts = crate::llm::GenOpts {
        max_tokens: Some(20),
        temperature: Some(0.0),
        json_schema: Some(schema),
        reasoning_effort: None,
    };
    let Ok(out) = llm.complete(msgs, opts).await else { return r };
    let label = serde_json::from_str::<serde_json::Value>(out.trim())
        .ok()
        .and_then(|v| v.get("task").and_then(|t| t.as_str()).map(str::to_string));
    let task = match label.as_deref() {
        Some("chat") => TaskType::Chat,
        Some("code") => TaskType::Code,
        Some("math") => TaskType::Math,
        Some("analysis") => TaskType::Analysis,
        Some("research") => TaskType::Research,
        Some("planning") => TaskType::Planning,
        Some("personal") => TaskType::Personal,
        Some("factual") => TaskType::Factual,
        _ => return r,
    };
    if task == r.task {
        return Route { confidence: 0.8, ..r };
    }
    let d = (r.difficulty - base_difficulty(r.task) + base_difficulty(task)).clamp(0.0, 1.0);
    Route { task, difficulty: d, confidence: 0.75, ..r }
}

/// Heuristische route.
pub fn route(text: &str) -> Route {
    let f = features(text);
    let (task, conf) = if f.small_talk {
        (TaskType::Chat, 0.9)
    } else if f.has_code {
        (TaskType::Code, 0.75)
    } else if f.research_words > 0 {
        (TaskType::Research, 0.7)
    } else if f.math_ops >= 2 || (f.math_ops >= 1 && f.has_numbers) {
        (TaskType::Math, 0.7)
    } else if f.planning_words > 0 {
        (TaskType::Planning, 0.65)
    } else if f.analysis_words > 0 {
        (TaskType::Analysis, 0.65)
    } else if f.personal {
        (TaskType::Personal, 0.6)
    } else if f.questions > 0 {
        (TaskType::Factual, 0.55)
    } else {
        (TaskType::Chat, 0.5)
    };
    let mut d = base_difficulty(task);
    d += (f.words as f64 / 400.0).min(0.2);
    d += 0.05 * (f.questions.saturating_sub(1)) as f64;
    d += 0.05 * f.analysis_words.min(3) as f64;
    Route { task, difficulty: d.clamp(0.0, 1.0), features: f, confidence: conf }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(&'static str);
    #[async_trait::async_trait]
    impl crate::llm::CogLlm for Fixed {
        async fn complete(
            &self,
            _: Vec<vortex_models::backend::Message>,
            _: crate::llm::GenOpts,
        ) -> Result<String, String> {
            Ok(self.0.into())
        }
    }

    #[tokio::test]
    async fn lm_label_only_overrides_doubtful_routes() {
        let doubt = route("de deployment van gisteren");
        assert!(doubt.confidence < LM_LABEL_BELOW);
        let r = refine_with_lm(&Fixed(r#"{"task":"planning"}"#), "de deployment van gisteren", doubt.clone()).await;
        assert_eq!(r.task, TaskType::Planning);
        let r = refine_with_lm(&Fixed("onzin"), "x", doubt.clone()).await;
        assert_eq!(r.task, doubt.task);
        let sure = route("hoi!");
        let r = refine_with_lm(&Fixed(r#"{"task":"code"}"#), "hoi!", sure).await;
        assert_eq!(r.task, TaskType::Chat);
    }

    #[test]
    fn routes_typical_questions() {
        assert_eq!(route("hoi!").task, TaskType::Chat);
        assert_eq!(route("Waarom faalt deze Rust-code met error[E0599]?").task, TaskType::Code);
        assert_eq!(route("Bereken 17 × 23 + 4").task, TaskType::Math);
        assert_eq!(route("Vergelijk de voor- en nadelen van SQLite en Postgres").task, TaskType::Analysis);
        assert_eq!(route("Zoek uit wat de laatste studies zeggen over QLoRA").task, TaskType::Research);
        assert_eq!(route("Wat is mijn favoriete editor?").task, TaskType::Personal);
        assert!(
            route("Analyseer en vergelijk waarom systeem A sneller is dan B, en bewijs het").difficulty
                > route("hoi").difficulty + 0.4
        );
    }
}
