//! Zelfverbeterende prompts en tools (W18.06).
//! - Prompt: kandidaat-systeemprompts uit eigen fouten (mislukte eval-taken, correcties van de
//!   gebruiker) → elke kandidaat A/B op de eval-suites → alleen een kandidaat die aantoonbaar wint
//!   (B, interval > 0) wordt een suggestie; de gebruiker past hem toe (nieuwe configversie).
//! - Tools: zelfgeschreven tools die fouten geven → herschreven met de fout-invoer als extra test
//!   → alleen bij groene tests een nieuwe versie.
// VX:W18.06

use serde_json::{Value, json};
use vortex_cognition::GenOpts;
use vortex_models::backend::Message;

use crate::api::AppState;
use crate::chat::Variant;

fn rows(state: &AppState, sql: &str) -> Vec<(String, String)> {
    state
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare(sql)?;
            let v = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

pub async fn optimize_prompt(state: &AppState, suites: &[String], k: usize) -> Result<Value, String> {
    let cfg = state.kernel.config().current();
    let current = cfg.file("prompts/system.md").unwrap_or("").to_string();
    let llm = state.chat.role_llm(&cfg, "chat", state.kernel.shutdown_token().child_token()).ok_or("geen model")?;
    let fails = rows(
        state,
        "SELECT r.task_id || ': ' || substr(r.detail, 1, 200), substr(r.output, 1, 300) FROM eval_results r JOIN eval_runs e ON e.id = r.run_id WHERE r.status = 'fail' ORDER BY e.started_at DESC LIMIT 15",
    );
    let corr = rows(
        state,
        "SELECT correction, rating FROM feedback WHERE rating = 'correction' AND correction IS NOT NULL ORDER BY created_at DESC LIMIT 10",
    );
    let ctx = format!(
        "Huidige systeemprompt:\n---\n{current}\n---\nMislukte eval-taken:\n{}\nCorrecties van de gebruiker:\n{}",
        fails.iter().map(|(a, b)| format!("- {a} → antwoord: {b}")).collect::<Vec<_>>().join("\n"),
        corr.iter().map(|(a, _)| format!("- {a}")).collect::<Vec<_>>().join("\n")
    );
    let out = llm
        .complete(
            vec![
                Message::system(format!("Verbeter de systeemprompt gericht op de fouten hieronder, zonder de rest te verslechteren. Geef {k} volledige kandidaten (niet te lang, Nederlands). JSON: {{\"candidates\": [{{\"prompt\": \"…\", \"why\": \"…\"}}]}}")),
                Message::user(ctx),
            ],
            GenOpts { max_tokens: Some(3000), temperature: Some(0.7), json_schema: None, reasoning_effort: None },
        )
        .await?;
    let v = vortex_agents::json::extract(&out).unwrap_or_default();
    let cands: Vec<(String, String)> = v["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| Some((c["prompt"].as_str()?.to_string(), c["why"].as_str().unwrap_or("").to_string())))
        .filter(|(p, _)| p.len() > 40)
        .take(k)
        .collect();
    if cands.is_empty() {
        return Err("het model gaf geen bruikbare kandidaten".into());
    }
    let base = Variant { name: "huidig".into(), ..Default::default() };
    let mut results = Vec::new();
    let mut best: Option<(f64, String, String, Value)> = None;
    for (i, (p, why)) in cands.into_iter().enumerate() {
        let r = crate::experiments::ab(
            state,
            suites,
            base.clone(),
            Variant { name: format!("kandidaat {}", i + 1), system_prompt: Some(p.clone()), ..Default::default() },
        )
        .await?;
        let o = r["overall"].clone();
        let (diff, lo, verdict) = (
            o["diff"].as_f64().unwrap_or(0.0),
            o["ci_low"].as_f64().unwrap_or(0.0),
            o["verdict"].as_str().unwrap_or("").to_string(),
        );
        results.push(json!({"candidate": i + 1, "why": why, "diff": diff, "ci_low": lo, "verdict": verdict}));
        if verdict == "B" && lo > 0.0 && best.as_ref().is_none_or(|b| diff > b.0) {
            best = Some((diff, p, why, o));
        }
    }
    let suggestion = match &best {
        Some((diff, p, why, _)) => crate::autonomy::monitor::suggest(
            state.kernel.events(),
            "prompt",
            &format!("Nieuwe systeemprompt wint aantoonbaar (+{:.1} pt op de evals): {why}", diff * 100.0),
            Some(&crate::autonomy::NewTask {
                title: "Verbeterde systeemprompt toepassen".into(),
                kind: "apply_prompt".into(),
                params: json!({"content": p}),
                priority: 2,
                needs_idle: Some(false),
                ..Default::default()
            }),
        ),
        None => None,
    };
    Ok(
        json!({"candidates": results, "winner": best.map(|b| json!({"diff": b.0, "why": b.2, "stats": b.3})), "suggestion": suggestion}),
    )
}

/// Zelfgeschreven tools met recente fouten opnieuw laten schrijven (tests + fout-invoer).
pub async fn improve_tools(state: &AppState) -> Result<Value, String> {
    let hub = state.tools.clone().ok_or("geen tools")?;
    let cfg = state.kernel.config().current();
    let llm = state.chat.role_llm(&cfg, "chat", state.kernel.shutdown_token().child_token()).ok_or("geen model")?;
    let fails = rows(
        state,
        "SELECT json_extract(payload, '$.tool'), json_extract(payload, '$.error.message') FROM events WHERE kind = 'tool.call.failed' AND ts >= datetime('now', '-7 days')",
    );
    let gen_dir = hub
        .cfg
        .skills_dir
        .clone()
        .unwrap_or_else(|| "config/skills".into())
        .parent()
        .map(|p| p.join("tools").join("generated"))
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut by: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (t, e) in fails {
        if hub.registry.get(&t).is_some_and(|x| x.contract().origin == "generated") {
            by.entry(t).or_default().push(e);
        }
    }
    for (tool, errs) in by.into_iter().filter(|(_, e)| e.len() >= 2) {
        let (Ok(code), Ok(spec)) = (
            std::fs::read_to_string(gen_dir.join(&tool).join("tool.py")),
            std::fs::read_to_string(gen_dir.join(&tool).join("tool.json")),
        ) else {
            continue;
        };
        let spec: Value = serde_json::from_str(&spec).unwrap_or_default();
        let ans = llm
            .complete(
                vec![
                    Message::system("Verbeter deze Python-tool (`def run(args)`) zodat de fouten niet meer optreden; houd het gedrag voor de bestaande tests gelijk. Geef alleen JSON: {\"code\": \"…\"}"),
                    Message::user(format!("Code:\n{code}\n\nFouten:\n{}\n\nTests:\n{}", errs.iter().take(5).cloned().collect::<Vec<_>>().join("\n"), spec["tests"])),
                ],
                GenOpts { max_tokens: Some(2000), temperature: Some(0.2), json_schema: None, reasoning_effort: None },
            )
            .await
            .unwrap_or_default();
        let Some(new_code) = vortex_agents::json::extract(&ans).and_then(|v| v["code"].as_str().map(String::from))
        else {
            continue;
        };
        let opts = vortex_tools::ExecOptions::default();
        let r = hub.executor.call(&format!("improve:{tool}:{}", vortex_types::EventId::new()), "create_tool", json!({"name": tool, "description": spec["description"], "input_schema": spec["input_schema"], "code": new_code, "tests": spec["tests"]}), &opts).await;
        out.push(json!({"tool": tool, "errors": errs.len(), "improved": r.ok, "detail": r.output.map(|o| o.data).or_else(|| r.error.map(|e| json!(e.message)))}));
    }
    Ok(json!({"tools": out, "summary": format!("{} tool(s) bekeken", out.len())}))
}
