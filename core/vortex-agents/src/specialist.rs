//! Specialist (W09.05): objectieve checks. Code: codeblokken echt draaien in de sandbox
//! (Python) + review; rekenen: het model schrijft een korte Python-berekening, de sandbox
//! rekent, en we vergelijken met het getal in het antwoord.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use vortex_cognition::GenOpts;
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub};

use crate::Llm;
use crate::json::extract;
use crate::roles::Roles;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpecialistReport {
    pub kind: String,
    /// Objectieve check gedraaid?
    pub ran: bool,
    /// Uitkomst van de objectieve check (None = niet gedraaid).
    pub passed: Option<bool>,
    pub output: String,
    pub issues: Vec<(String, String)>,
}

/// Codeblokken met taal.
pub fn code_blocks(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("```") {
        let after = &rest[i + 3..];
        let Some(nl) = after.find('\n') else { break };
        let lang = after[..nl].trim().to_lowercase();
        let body = &after[nl + 1..];
        let Some(end) = body.find("```") else { break };
        out.push((lang, body[..end].to_string()));
        rest = &body[end + 3..];
    }
    out
}

/// Laatste getal in een tekst (voor rekenchecks).
pub fn last_number(s: &str) -> Option<f64> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for ch in s.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || (ch == '-' && cur.is_empty()) || ((ch == '.' || ch == ',') && !cur.is_empty()) {
            cur.push(if ch == ',' { '.' } else { ch });
        } else {
            let t = cur.trim_end_matches('.').to_string();
            if let Ok(v) = t.parse::<f64>() {
                nums.push(v);
            }
            cur.clear();
        }
    }
    nums.pop()
}

async fn run_python(tools: &ToolHub, exec: &ExecOptions, key: &str, code: &str) -> (bool, String) {
    let r = tools.executor.call(key, "python", json!({"code": code, "timeout_s": 30}), exec).await;
    (r.ok, r.for_model())
}

pub async fn code(
    llm: &Llm,
    roles: &Roles,
    tools: Option<&Arc<ToolHub>>,
    exec: &ExecOptions,
    key: &str,
    answer: &str,
) -> SpecialistReport {
    let mut rep = SpecialistReport { kind: "code".into(), ..Default::default() };
    if let Some(t) = tools {
        let py: Vec<String> =
            code_blocks(answer).into_iter().filter(|(l, _)| l == "python" || l == "py").map(|(_, c)| c).collect();
        if !py.is_empty() {
            let mut all = true;
            let mut outs = Vec::new();
            for (k, c) in py.iter().enumerate() {
                let (ok, out) = run_python(t, exec, &format!("{key}:py{k}"), c).await;
                all &= ok;
                outs.push(out.chars().take(800).collect::<String>());
            }
            rep.ran = true;
            rep.passed = Some(all);
            rep.output = outs.join("\n---\n");
        }
    }
    let r = roles.get("specialist-code");
    let mut prompt = format!("Antwoord met code:\n{}\n", answer.chars().take(8000).collect::<String>());
    if rep.ran {
        prompt.push_str(&format!(
            "\nTestresultaat (code echt uitgevoerd): {}\n{}\n",
            if rep.passed == Some(true) { "GESLAAGD" } else { "MISLUKT" },
            rep.output
        ));
    }
    rep.issues = issues(llm, &r.prompt, r.temperature, &prompt).await;
    rep
}

pub async fn math(
    llm: &Llm,
    tools: Option<&Arc<ToolHub>>,
    exec: &ExecOptions,
    key: &str,
    question: &str,
    answer: &str,
) -> SpecialistReport {
    let mut rep = SpecialistReport { kind: "math".into(), ..Default::default() };
    let Some(t) = tools else { return rep };
    let schema = json!({"type": "object", "required": ["code"], "properties": {"code": {"type": "string"}}});
    let out = llm
        .complete(
            vec![
                Message::system("Schrijf een kort Python-programma dat het gevraagde getal uitrekent en alleen dat getal print. Geen uitleg. Antwoord als JSON: {\"code\": \"…\"}."),
                Message::user(question.to_string()),
            ],
            GenOpts { max_tokens: Some(500), temperature: Some(0.0), json_schema: Some(schema), reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    let Some(code) = extract(&out).and_then(|v| v["code"].as_str().map(String::from)) else { return rep };
    let (ok, printed) = run_python(t, exec, &format!("{key}:math"), &code).await;
    rep.ran = ok;
    rep.output = printed.clone();
    if ok && let (Some(want), Some(got)) = (last_number(&printed), last_number(answer)) {
        let same = (want - got).abs() <= 1e-6_f64.max(want.abs() * 1e-6);
        rep.passed = Some(same);
        if !same {
            rep.issues.push(("high".into(), format!("berekening geeft {want}, het antwoord zegt {got}")));
        }
    }
    rep
}

/// Issues-lijst van een rol die JSON {issues:[{severity,text}]} geeft.
pub async fn issues(llm: &Llm, system: &str, temperature: Option<f32>, user: &str) -> Vec<(String, String)> {
    let schema = json!({"type": "object", "required": ["issues"], "properties": {"issues": {"type": "array", "items": {"type": "object", "required": ["severity", "text"], "properties": {"severity": {"type": "string", "enum": ["high", "medium", "low"]}, "text": {"type": "string"}}}}}});
    let out = llm
        .complete(
            vec![Message::system(system.to_string()), Message::user(user.to_string())],
            GenOpts { max_tokens: Some(700), temperature, json_schema: Some(schema), reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    extract(&out)
        .and_then(|v| v["issues"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| Some((x["severity"].as_str()?.to_string(), x["text"].as_str()?.to_string())))
        .take(8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let b = code_blocks("tekst\n```python\nprint(1)\n```\nen\n```rust\nfn x(){}\n```");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], ("python".into(), "print(1)\n".into()));
        assert_eq!(last_number("Het antwoord is 76127."), Some(76127.0));
        assert_eq!(last_number("som = 3,5 en daarna 12"), Some(12.0));
        assert_eq!(last_number("geen"), None);
    }
}
