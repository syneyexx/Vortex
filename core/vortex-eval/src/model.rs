//! Eval-taken en -sets (JSONL in `evals/sets/`).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Eén automatische controle op een antwoord.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Check {
    /// Antwoord bevat (hoofdletterongevoelig) alle termen.
    Contains {
        all: Vec<String>,
    },
    /// Antwoord bevat minstens één van de termen.
    ContainsAny {
        any: Vec<String>,
    },
    /// Antwoord bevat geen van de termen.
    NotContains {
        none: Vec<String>,
    },
    Regex {
        pattern: String,
    },
    /// Laatste getal in het antwoord ligt binnen `tolerance` van `value`.
    Number {
        value: f64,
        #[serde(default)]
        tolerance: f64,
    },
    /// Python-code uit het antwoord + `tests` moet slagen (exitcode 0).
    PythonTests {
        tests: String,
    },
    /// Antwoord is geldige JSON met deze velden.
    JsonFields {
        fields: Vec<String>,
    },
    /// Antwoord verwijst naar een van deze bronnen (bv. "[bron: x]" of de naam).
    CitesSource {
        any: Vec<String>,
    },
    /// Antwoord geeft eerlijk aan het niet te weten.
    AdmitsUnknown,
    /// Aantal open geheugenconflicten na de taak.
    ConflictsAtLeast {
        n: usize,
    },
}

/// Eén taak.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalTask {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Feiten die vooraf in een geïsoleerde eval-werkruimte worden onthouden.
    #[serde(default)]
    pub memory: Vec<String>,
    /// Benodigde wave/functie; ontbreekt die nog → `skip` (eerlijk, niet als fout).
    #[serde(default)]
    pub requires: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub difficulty: Option<u8>,
    /// Counterfactuals (W17.04): zelfde taak met één wezenlijke wijziging → ander verwacht antwoord.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counterfactual: Vec<Counterfactual>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Counterfactual {
    pub prompt: String,
    pub checks: Vec<Check>,
}

/// Een suite = alle taken uit één bestand.
#[derive(Debug, Clone, Serialize)]
pub struct Suite {
    pub name: String,
    pub tasks: Vec<EvalTask>,
}

pub fn load_suite(path: &Path) -> Result<Suite, String> {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut tasks = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        let t: EvalTask = serde_json::from_str(l).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?;
        if tasks.iter().any(|x: &EvalTask| x.id == t.id) {
            return Err(format!("{}: dubbel task-id {}", path.display(), t.id));
        }
        tasks.push(t);
    }
    Ok(Suite { name, tasks })
}

/// Alle suites in een map (behalve `retrieval*.json`, die apart gaat).
pub fn load_dir(dir: &Path) -> Result<Vec<Suite>, String> {
    let mut out = Vec::new();
    let mut paths: Vec<_> =
        std::fs::read_dir(dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(load_suite(&p)?);
        }
    }
    Ok(out)
}

/// Retrieval-set: een corpus + vragen met de relevante documenten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalSet {
    pub corpus: Vec<RetrievalDoc>,
    pub questions: Vec<RetrievalQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalDoc {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalQuestion {
    pub q: String,
    #[serde(default)]
    pub relevant: Vec<String>,
    /// Er is geen antwoord in het corpus (systeem hoort niets relevants te vinden).
    #[serde(default)]
    pub no_answer: bool,
}
