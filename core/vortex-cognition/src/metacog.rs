//! Metacognitie-controller (W07.04): snel of diep, redeneerniveau, planstrategie, budget,
//! wanneer stoppen, wanneer de gebruiker iets vragen; dynamisch wisselen van strategie bij
//! stall-signalen; selective prediction (antwoorden / meer bewijs / vragen / geen conclusie).
// VX:W07.04

use serde::{Deserialize, Serialize};

use crate::router::{Route, TaskType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Path {
    /// System 1: één generatie.
    Fast,
    /// System 2: plannen, uitvoeren, kritisch nalopen.
    Deliberate,
    /// Research-pipeline (W10); tot dan: Deliberate met eerlijke beperking.
    Research,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStyle {
    Direct,
    PlanExecute,
    Reflexion,
    ReAct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prediction {
    Answer,
    MoreEvidence,
    AskUser,
    Abstain,
}

/// Wat de controller ziet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signals {
    pub route: Route,
    /// Keuze van de gebruiker in de invoerbalk ("adaptive" = metacognitie beslist).
    pub mode: String,
    /// Beste geheugenscore (0..1), of None als niets gevonden.
    pub best_memory: Option<f64>,
    /// Open conflicten bij de opgehaalde herinneringen.
    pub conflicts: usize,
    /// Tools beschikbaar (W08).
    pub tools: bool,
    /// Research-pipeline beschikbaar (W10).
    pub research: bool,
    /// Zelfmodel (W12.06): eval-score voor dit taaktype, als bekend.
    #[serde(default)]
    pub self_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub path: Path,
    pub plan: PlanStyle,
    pub reasoning_effort: String,
    pub max_steps: u32,
    pub prediction: Prediction,
    /// Extra instructies voor de prompt (eerlijkheid, tegenstrijdigheid, bronnen).
    pub guidance: Vec<String>,
    /// Vraag aan de gebruiker (bij AskUser).
    pub ask: Option<String>,
    pub why: Vec<String>,
}

/// Beslis de strategie.
pub fn decide(s: &Signals) -> Strategy {
    let r = &s.route;
    let mut why = Vec::new();
    let mut guidance = Vec::new();
    let mut path = match s.mode.as_str() {
        "quick" => {
            why.push("gebruiker koos Quick".into());
            Path::Fast
        }
        "deep" => {
            why.push("gebruiker koos Deep".into());
            Path::Deliberate
        }
        "research" => {
            why.push("gebruiker koos Research".into());
            Path::Research
        }
        _ => {
            let p = if r.task == TaskType::Research {
                Path::Research
            } else if r.difficulty >= 0.55 {
                Path::Deliberate
            } else {
                Path::Fast
            };
            why.push(format!("adaptive: {:?}, moeilijkheid {:.2}", r.task, r.difficulty));
            p
        }
    };
    if path == Path::Research && !s.research {
        guidance.push("Je hebt nog geen webtoegang of research-pipeline: baseer je op eigen kennis en geheugen en zeg duidelijk wat je niet kunt controleren.".into());
        why.push("research-pipeline nog niet beschikbaar (W10) → diep nadenken".into());
        path = Path::Deliberate;
    }
    // Zelfmodel (W12.06): zwak in dit taaktype → niet op de automatische piloot.
    if let Some(sc) = s.self_score
        && sc < crate::selfmodel::WEAK
    {
        guidance.push(format!("Volgens de eigen eval-resultaten is dit een zwak taaktype ({:.0}%): werk extra zorgvuldig, controleer tussenstappen en zeg wat onzeker is.", sc * 100.0));
        why.push(format!("zelfmodel: zwak in {:?} ({:.0}%)", r.task, sc * 100.0));
        if path == Path::Fast && s.mode != "quick" {
            path = Path::Deliberate;
        }
    }
    let plan = match (path, r.task) {
        (Path::Fast, _) => PlanStyle::Direct,
        (_, TaskType::Code) | (_, TaskType::Math) => PlanStyle::Reflexion,
        (_, _) if s.tools && r.features.has_numbers => PlanStyle::ReAct,
        _ => PlanStyle::PlanExecute,
    };
    let reasoning_effort = match (path, r.difficulty) {
        (Path::Fast, d) if d < 0.2 => "low",
        (Path::Fast, _) => "medium",
        _ => "high",
    }
    .to_string();
    // Selective prediction.
    let mut prediction = Prediction::Answer;
    let ask = None;
    if r.task == TaskType::Personal && s.best_memory.is_none_or(|b| b < 0.35) {
        prediction = Prediction::Abstain;
        guidance.push("Dit gaat over de gebruiker en je geheugen bevat er niets betrouwbaars over: verzin niets. Zeg eerlijk dat je het niet weet en vraag het eventueel na.".into());
        why.push("persoonlijke vraag zonder geheugen → niet gokken".into());
    }
    if s.conflicts > 0 {
        prediction = Prediction::MoreEvidence;
        guidance.push("Je geheugen bevat tegenstrijdige informatie hierover: benoem beide versies en zeg welke onzeker is, in plaats van er één te kiezen.".into());
        why.push(format!("{} conflict(en) in geheugen", s.conflicts));
    }
    if r.features.words <= 3 && !r.features.small_talk {
        // Korte input is vaak een vervolg ("ja", "doorgaan") of een gewone vraag: niet blokkeren,
        // wel het model laten doorvragen als het echt onduidelijk is.
        guidance.push(
            "Als de vraag zonder context onduidelijk is, vraag dan kort wat er bedoeld wordt in plaats van te gokken."
                .into(),
        );
    }
    if r.features.time_sensitive && !s.research {
        guidance.push(
            "De vraag is tijdgevoelig en je kunt niet online controleren: geef aan tot wanneer je kennis loopt.".into(),
        );
    }
    let max_steps = match path {
        Path::Fast => 1,
        Path::Deliberate => 4,
        Path::Research => 8,
    };
    Strategy { path, plan, reasoning_effort, max_steps, prediction, guidance, ask, why }
}

/// Dynamisch wisselen bij stall-signalen of mislukte verificatie.
pub fn switch_on(stall: &str, current: &Strategy) -> Option<Strategy> {
    let mut next = current.clone();
    match (current.plan, stall) {
        (PlanStyle::PlanExecute, "plan_churn" | "repeated_action") => {
            next.plan = PlanStyle::Reflexion;
            next.why.push(format!("wissel: {stall} → reflexion"));
        }
        (PlanStyle::Direct, "verify_failed") => {
            next.path = Path::Deliberate;
            next.plan = PlanStyle::Reflexion;
            next.reasoning_effort = "high".into();
            next.why.push("verificatie faalde → diep".into());
        }
        (_, "no_progress") => {
            next.prediction = Prediction::AskUser;
            next.ask = Some("Ik kom niet verder. Kun je de vraag anders formuleren of meer context geven?".into());
            next.why.push("geen voortgang → gebruiker vragen".into());
        }
        _ => return None,
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::route;

    fn sig(q: &str, mode: &str) -> Signals {
        Signals {
            self_score: None,
            route: route(q),
            mode: mode.into(),
            best_memory: Some(0.8),
            conflicts: 0,
            tools: false,
            research: false,
        }
    }

    #[test]
    fn gate_adaptive_picks_right_path() {
        // (vraag, verwacht pad) — de eval "Adaptive kiest het juiste pad".
        let cases = [
            ("hoi", Path::Fast),
            ("Wat is de hoofdstad van Frankrijk?", Path::Fast),
            ("Wat is mijn favoriete editor?", Path::Fast),
            (
                "Analyseer waarom onze retrieval traag is en vergelijk drie oplossingen met voor- en nadelen",
                Path::Deliberate,
            ),
            (
                "Schrijf een Rust-functie die intervallen samenvoegt en leg uit waarom het O(n log n) is",
                Path::Deliberate,
            ),
            ("Zoek uit wat de laatste studies zeggen over QLoRA", Path::Deliberate), // research → deliberate tot W10
        ];
        let mut ok = 0;
        for (q, want) in cases {
            let d = decide(&sig(q, "adaptive"));
            if d.path == want {
                ok += 1;
            } else {
                eprintln!("fout: {q} → {:?} ({:?})", d.path, d.why);
            }
        }
        assert_eq!(ok, cases.len());
    }

    #[test]
    fn overrides_and_selective_prediction() {
        assert_eq!(decide(&sig("Analyseer en vergelijk alles uitvoerig", "quick")).path, Path::Fast);
        assert_eq!(decide(&sig("hoi", "deep")).path, Path::Deliberate);
        let mut s = sig("Wat is de naam van mijn hond?", "adaptive");
        s.best_memory = None;
        let d = decide(&s);
        assert_eq!(d.prediction, Prediction::Abstain);
        assert!(d.guidance.iter().any(|g| g.contains("verzin niets")));
        let mut c = sig("Waar woon ik?", "adaptive");
        c.conflicts = 1;
        assert_eq!(decide(&c).prediction, Prediction::MoreEvidence);
        let k = decide(&sig("kat", "adaptive"));
        assert_eq!(k.prediction, Prediction::Answer, "kort ≠ blokkeren");
        assert!(k.guidance.iter().any(|g| g.contains("vraag dan kort")));
    }

    #[test]
    fn strategy_switching() {
        let d = decide(&sig("Analyseer waarom X en vergelijk met Y uitgebreid", "adaptive"));
        assert_eq!(d.plan, PlanStyle::PlanExecute);
        assert_eq!(switch_on("plan_churn", &d).map(|x| x.plan), Some(PlanStyle::Reflexion));
        let f = decide(&sig("Wat is 2+2?", "quick"));
        assert_eq!(switch_on("verify_failed", &f).map(|x| x.path), Some(Path::Deliberate));
        assert!(switch_on("onbekend", &f).is_none());
    }
}
