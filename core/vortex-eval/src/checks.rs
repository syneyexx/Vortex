//! Automatische scoring van één antwoord.

use std::time::Duration;

use crate::model::Check;

/// Uitslag van één check.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub passed: bool,
    pub detail: String,
}

fn ok(passed: bool, detail: impl Into<String>) -> CheckResult {
    CheckResult { passed, detail: detail.into() }
}

/// Laatste getal in een tekst (komma of punt als decimaalteken).
pub fn last_number(text: &str) -> Option<f64> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || ((c == '.' || c == ',') && !cur.is_empty()) || (c == '-' && cur.is_empty()) {
            cur.push(if c == ',' { '.' } else { c });
        } else if !cur.is_empty() {
            let t = cur.trim_end_matches('.').to_string();
            if let Ok(v) = t.parse::<f64>() {
                nums.push(v);
            }
            cur.clear();
        }
    }
    nums.last().copied()
}

/// Eerste codeblok (```python ... ``` of ``` ... ```), anders de hele tekst.
pub fn code_block(text: &str) -> String {
    if let Some(start) = text.find("```") {
        let rest = &text[start + 3..];
        let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or(rest);
        if let Some(end) = rest.find("```") {
            return rest[..end].to_string();
        }
    }
    text.to_string()
}

const UNKNOWN: &[&str] = &[
    "weet ik niet",
    "weet het niet",
    "niet bekend",
    "geen informatie",
    "kan ik niet vinden",
    "niet zeker",
    "onbekend",
    "i don't know",
    "not sure",
    "no information",
    "geen gegevens",
    "kan dat niet zeggen",
    "heb ik niet",
];

/// Voer een check uit. `python` = pad naar de interpreter; `open_conflicts` uit het geheugen.
pub async fn run_check(check: &Check, answer: &str, python: &str, open_conflicts: usize) -> CheckResult {
    let low = answer.to_lowercase();
    match check {
        Check::Contains { all } => {
            let miss: Vec<&String> = all.iter().filter(|t| !low.contains(&t.to_lowercase())).collect();
            ok(miss.is_empty(), if miss.is_empty() { "alles gevonden".into() } else { format!("mist: {miss:?}") })
        }
        Check::ContainsAny { any } => {
            let hit = any.iter().find(|t| low.contains(&t.to_lowercase()));
            ok(hit.is_some(), format!("gevonden: {hit:?}"))
        }
        Check::NotContains { none } => {
            let hit: Vec<&String> = none.iter().filter(|t| low.contains(&t.to_lowercase())).collect();
            ok(hit.is_empty(), format!("verboden gevonden: {hit:?}"))
        }
        Check::Regex { pattern } => match regex::Regex::new(pattern) {
            Ok(r) => ok(r.is_match(answer), format!("/{pattern}/")),
            Err(e) => ok(false, format!("ongeldige regex: {e}")),
        },
        Check::Number { value, tolerance } => match last_number(answer) {
            Some(v) => ok((v - value).abs() <= tolerance.max(1e-9), format!("gevonden {v}, verwacht {value}")),
            None => ok(false, "geen getal in antwoord"),
        },
        Check::JsonFields { fields } => {
            let raw = code_block(answer);
            match serde_json::from_str::<serde_json::Value>(raw.trim()) {
                Ok(v) => {
                    let miss: Vec<&String> = fields.iter().filter(|f| v.get(f.as_str()).is_none()).collect();
                    ok(miss.is_empty(), format!("ontbrekend: {miss:?}"))
                }
                Err(e) => ok(false, format!("geen JSON: {e}")),
            }
        }
        Check::CitesSource { any } => {
            let hit = any.iter().find(|t| low.contains(&t.to_lowercase()));
            ok(hit.is_some(), format!("bron: {hit:?}"))
        }
        Check::AdmitsUnknown => {
            let hit = UNKNOWN.iter().find(|t| low.contains(**t));
            ok(hit.is_some(), format!("{hit:?}"))
        }
        Check::ConflictsAtLeast { n } => ok(open_conflicts >= *n, format!("{open_conflicts} open conflicten")),
        Check::PythonTests { tests } => {
            let code = format!("{}\n\n{}\n", code_block(answer), tests);
            let mut cmd = tokio::process::Command::new(python);
            cmd.arg("-I")
                .arg("-c")
                .arg(&code)
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            match cmd.spawn() {
                Ok(child) => match tokio::time::timeout(Duration::from_secs(10), child.wait_with_output()).await {
                    Ok(Ok(o)) => {
                        ok(o.status.success(), String::from_utf8_lossy(&o.stderr).chars().take(400).collect::<String>())
                    }
                    Ok(Err(e)) => ok(false, e.to_string()),
                    Err(_) => ok(false, "timeout (10 s)"),
                },
                Err(e) => ok(false, format!("python niet gestart: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_code() {
        assert_eq!(last_number("Het antwoord is 3,5 kg."), Some(3.5));
        assert_eq!(last_number("eerst 2 dan 42"), Some(42.0));
        assert_eq!(last_number("geen"), None);
        assert_eq!(code_block("tekst\n```python\nx=1\n```\nmeer"), "x=1\n");
    }

    #[tokio::test]
    async fn checks_work() {
        let py = if std::process::Command::new("python3").arg("-V").output().is_ok() { "python3" } else { "python" };
        assert!(run_check(&Check::Contains { all: vec!["Rust".into()] }, "ik gebruik rust", py, 0).await.passed);
        assert!(!run_check(&Check::NotContains { none: vec!["x".into()] }, "x", py, 0).await.passed);
        assert!(run_check(&Check::Number { value: 12.0, tolerance: 0.0 }, "= 12", py, 0).await.passed);
        assert!(run_check(&Check::AdmitsUnknown, "Dat weet ik niet zeker.", py, 0).await.passed);
        let good = "```python\ndef add(a, b):\n    return a + b\n```";
        assert!(run_check(&Check::PythonTests { tests: "assert add(2, 3) == 5".into() }, good, py, 0).await.passed);
        let bad = "```python\ndef add(a, b):\n    return a - b\n```";
        assert!(!run_check(&Check::PythonTests { tests: "assert add(2, 3) == 5".into() }, bad, py, 0).await.passed);
        assert!(run_check(&Check::JsonFields { fields: vec!["a".into()] }, "{\"a\": 1}", py, 0).await.passed);
    }
}
