//! Planning (W07.05): plan-and-execute (hiërarchisch via de doelstapel), reflexion
//! (zelfkritiek → herzien) en ReAct (denken → actie via tool → waarneming).
// VX:W07.05

use serde::{Deserialize, Serialize};
use vortex_context::{Budget, assemble_for_role};
use vortex_models::backend::Message;

use crate::llm::{CogLlm, GenOpts};
use crate::workspace::{GoalStatus, Workspace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<String>,
}

const PLANNER: &str = "Je bent de Planner van Vortex. Splits de taak in 2 tot 5 concrete, controleerbare stappen. Geen stap voor 'antwoord schrijven' — dat gebeurt apart. Alleen JSON.";
const EXECUTOR: &str = "Je voert één stap uit van een plan. Werk alleen deze stap uit, kort en feitelijk. Gebruik de eerdere resultaten. Zeg eerlijk wat je niet weet.";
const CRITIC: &str = "Je bent de Critic van Vortex. Zoek fouten, gaten, onbewezen claims en rekenfouten in het concept. Alleen JSON: {\"verdict\":\"ok\"|\"revise\",\"issues\":[...]}.";

fn plan_schema() -> serde_json::Value {
    serde_json::json!({"type":"object","properties":{"steps":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":5}},"required":["steps"]})
}

fn critique_schema() -> serde_json::Value {
    serde_json::json!({"type":"object","properties":{"verdict":{"type":"string","enum":["ok","revise"]},"issues":{"type":"array","items":{"type":"string"}}},"required":["verdict","issues"]})
}

// ── W17.12: CSE, incrementele herberekening, value-of-information ─────────────
// VX:W17.12

fn words(s: &str) -> std::collections::BTreeSet<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect()
}

/// Common subexpression elimination: (bijna) gelijke stappen maar één keer (Jaccard ≥ 0,8).
pub fn dedup_steps(steps: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in steps {
        let w = words(&s);
        let dup = out.iter().any(|o| {
            let ow = words(o);
            let inter = w.intersection(&ow).count() as f64;
            let uni = w.union(&ow).count().max(1) as f64;
            inter / uni >= 0.8
        });
        if !dup {
            out.push(s);
        }
    }
    out
}

/// Stapresultaten hergebruiken als stap + invoer (eerdere resultaten) niet veranderd zijn
/// (incrementele herberekening bij herplannen/reflexion, ook over runs heen binnen de TTL).
#[derive(Debug, Default)]
pub struct StepCache {
    inner: parking_lot::Mutex<std::collections::HashMap<u64, (std::time::Instant, String)>>,
}

impl StepCache {
    pub const TTL: std::time::Duration = std::time::Duration::from_secs(1800);

    pub fn key(task: &str, step: &str, material: &[String]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for part in std::iter::once(task).chain(std::iter::once(step)).chain(material.iter().map(|s| s.as_str())) {
            for b in part.bytes().chain(std::iter::once(0x1e)) {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }

    pub fn get(&self, k: u64) -> Option<String> {
        let g = self.inner.lock();
        g.get(&k).filter(|(t, _)| t.elapsed() < Self::TTL).map(|(_, v)| v.clone())
    }

    pub fn put(&self, k: u64, v: &str) {
        let mut g = self.inner.lock();
        if g.len() > 2000 {
            g.retain(|_, (t, _)| t.elapsed() < Self::TTL);
        }
        g.insert(k, (std::time::Instant::now(), v.to_string()));
    }
}

/// Value-of-information: een opzoek-/controlestap levert weinig op als het geheugen het al
/// met hoge zekerheid weet (`known` = beste geheugenscore 0..1).
pub fn low_value(step: &str, known: f64) -> bool {
    let l = step.to_lowercase();
    let lookup = ["zoek", "opzoeken", "controleer", "verifieer", "achterhaal", "check"].iter().any(|w| l.contains(w));
    lookup && known >= 0.85
}

/// Plan maken (doelen in de workspace). Mislukt het JSON → één stap = de vraag zelf.
pub async fn make_plan(llm: &dyn CogLlm, ws: &Workspace, task: &str, context: &str) -> Plan {
    let msgs = vec![Message::system(PLANNER), Message::user(format!("Taak: {task}\n\nContext:\n{context}"))];
    let opts = GenOpts {
        json_schema: Some(plan_schema()),
        max_tokens: Some(400),
        temperature: Some(0.2),
        ..Default::default()
    };
    let plan = match llm.complete(msgs, opts).await.ok().and_then(|s| serde_json::from_str::<Plan>(&s).ok()) {
        Some(p) if !p.steps.is_empty() => Plan { steps: dedup_steps(p.steps).into_iter().take(5).collect() },
        _ => Plan { steps: vec![task.to_string()] },
    };
    let mut prev: Option<usize> = None;
    for s in &plan.steps {
        let id = ws.push_goal(s, None, prev.into_iter().collect());
        prev = Some(id);
    }
    plan
}

/// Voer de klaarstaande doelen uit, elk met een geïsoleerde context (alleen taak + eerdere
/// resultaten). `on_step` krijgt (index, stap, resultaat) voor voortgang/events/checkpoints.
pub async fn execute_plan(
    llm: &dyn CogLlm,
    ws: &Workspace,
    task: &str,
    max_steps: u32,
    on_step: impl FnMut(usize, &str, &str),
) -> Vec<(String, String)> {
    execute_plan_with(llm, ws, task, max_steps, None, 0.0, on_step).await
}

/// Zoals [`execute_plan`], met stapcache (incrementeel) en VOI (`known` = geheugenzekerheid).
pub async fn execute_plan_with(
    llm: &dyn CogLlm,
    ws: &Workspace,
    task: &str,
    max_steps: u32,
    cache: Option<&StepCache>,
    known: f64,
    mut on_step: impl FnMut(usize, &str, &str),
) -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();
    let mut n: usize = 0;
    while let Some(g) = ws.ready_goals().into_iter().next() {
        if n >= max_steps as usize {
            ws.add_uncertainty("stapbudget op: niet alle planstappen uitgevoerd");
            break;
        }
        ws.set_goal_status(g.id, GoalStatus::Active, None);
        let material: Vec<String> = results.iter().map(|(s, r)| format!("Stap '{s}': {r}")).collect();
        if low_value(&g.text, known) {
            let out =
                "(overgeslagen: het geheugen weet dit al met hoge zekerheid — weinig informatiewaarde)".to_string();
            ws.set_goal_status(g.id, GoalStatus::Done, Some(out.clone()));
            on_step(n, &g.text, &out);
            results.push((g.text.clone(), out));
            n += 1;
            continue;
        }
        let key = StepCache::key(task, &g.text, &material);
        if let Some(out) = cache.and_then(|c| c.get(key)) {
            ws.add_note(&format!("stap hergebruikt (invoer ongewijzigd): {}", g.text));
            ws.set_goal_status(g.id, GoalStatus::Done, Some(out.chars().take(500).collect()));
            on_step(n, &g.text, &out);
            results.push((g.text.clone(), out));
            n += 1;
            continue;
        }
        let a = assemble_for_role(
            EXECUTOR,
            &format!("Hoofdtaak: {task}\nHuidige stap: {}", g.text),
            &material,
            &Budget { window: 16_384, answer: 1_024, ..Default::default() },
        );
        let out = llm
            .complete(a.messages, GenOpts { max_tokens: Some(700), temperature: Some(0.3), ..Default::default() })
            .await
            .unwrap_or_else(|e| format!("(stap mislukt: {e})"));
        if let Some(c) = cache
            && !out.starts_with("(stap mislukt")
        {
            c.put(key, &out);
        }
        ws.set_goal_status(g.id, GoalStatus::Done, Some(out.chars().take(500).collect()));
        on_step(n, &g.text, &out);
        results.push((g.text.clone(), out));
        n += 1;
    }
    results
}

/// Featurevector voor de PRM-layer (process reward model, W13.05) per planstap.
pub fn prm_vector(i: usize, n: usize, question: &str, output: &str, flagged: bool) -> Vec<f32> {
    let l = output.to_lowercase();
    let hedges =
        ["misschien", "waarschijnlijk", "onzeker", "mogelijk", "weet niet"].iter().filter(|w| l.contains(*w)).count()
            as f32;
    let qt: std::collections::HashSet<String> =
        question.to_lowercase().split_whitespace().filter(|w| w.len() > 3).map(String::from).collect();
    let ot: std::collections::HashSet<String> =
        l.split_whitespace().filter(|w| w.len() > 3).map(String::from).collect();
    let overlap = qt.intersection(&ot).count() as f32 / qt.len().max(1) as f32;
    vec![
        i as f32 / n.max(1) as f32,
        (output.chars().count() as f32 / 1000.0).min(10.0),
        if output.chars().any(|c| c.is_ascii_digit()) { 1.0 } else { 0.0 },
        hedges,
        overlap,
        if flagged { 1.0 } else { 0.0 },
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Critique {
    pub verdict: String,
    pub issues: Vec<String>,
}

/// Reflexion: concept laten bekritiseren (aparte, geïsoleerde rol).
pub async fn critique(llm: &dyn CogLlm, task: &str, draft: &str) -> Critique {
    let msgs = vec![Message::system(CRITIC), Message::user(format!("Taak:\n{task}\n\nConcept:\n{draft}"))];
    let opts = GenOpts {
        json_schema: Some(critique_schema()),
        max_tokens: Some(400),
        temperature: Some(0.1),
        ..Default::default()
    };
    llm.complete(msgs, opts)
        .await
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Critique { verdict: "ok".into(), issues: vec![] })
}

/// Tool-aanroep in een ReAct-lus (de toollaag in W08 implementeert dit).
#[async_trait::async_trait]
pub trait ToolRunner: Send + Sync {
    fn tools(&self) -> Vec<vortex_models::backend::ToolSpec>;
    async fn run(&self, name: &str, args: serde_json::Value) -> Result<String, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    pub thought: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
    #[serde(default)]
    pub answer: Option<String>,
}

/// ReAct: denken → (tool) → waarnemen, tot een antwoord of het stapbudget op is.
pub async fn react(
    llm: &dyn CogLlm,
    tools: &dyn ToolRunner,
    task: &str,
    max_steps: u32,
    mut on_step: impl FnMut(&ReActStep, Option<&str>),
) -> Result<String, String> {
    let list: String = tools
        .tools()
        .iter()
        .map(|t| format!("- {}: {} (args: {})", t.name, t.description, t.parameters))
        .collect::<Vec<_>>()
        .join("\n");
    let schema = serde_json::json!({"type":"object","properties":{"thought":{"type":"string"},"action":{"type":["string","null"]},"args":{"type":["object","null"]},"answer":{"type":["string","null"]}},"required":["thought"]});
    let mut transcript = String::new();
    for _ in 0..max_steps {
        let msgs = vec![
            Message::system(format!(
                "Los de taak op met denkstappen en tools. Per stap JSON: thought + óf action/args óf answer.\nTools:\n{list}"
            )),
            Message::user(format!("Taak: {task}\n\nTot nu toe:\n{transcript}")),
        ];
        let raw = llm
            .complete(
                msgs,
                GenOpts {
                    json_schema: Some(schema.clone()),
                    max_tokens: Some(500),
                    temperature: Some(0.2),
                    ..Default::default()
                },
            )
            .await?;
        let step: ReActStep = serde_json::from_str(&raw).map_err(|e| format!("ongeldige stap: {e}"))?;
        if let Some(a) = &step.answer {
            on_step(&step, None);
            return Ok(a.clone());
        }
        let Some(action) = step.action.clone() else {
            return Err("stap zonder actie en zonder antwoord".into());
        };
        let obs = tools
            .run(&action, step.args.clone().unwrap_or(serde_json::json!({})))
            .await
            .unwrap_or_else(|e| format!("FOUT: {e}"));
        on_step(&step, Some(&obs));
        transcript.push_str(&format!(
            "Gedachte: {}\nActie: {action} {}\nWaarneming: {}\n",
            step.thought,
            step.args.unwrap_or_default(),
            obs.chars().take(2000).collect::<String>()
        ));
    }
    Err("stapbudget op zonder antwoord".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vortex_types::{RunId, TraceId};

    #[derive(Debug)]
    struct Script(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl CogLlm for Script {
        async fn complete(&self, _m: Vec<Message>, _o: GenOpts) -> Result<String, String> {
            let mut v = self.0.lock().map_err(|e| e.to_string())?;
            if v.is_empty() { Err("leeg".into()) } else { Ok(v.remove(0)) }
        }
    }

    #[tokio::test]
    async fn plan_and_execute_uses_goal_stack() {
        let llm = Script(Mutex::new(vec![
            r#"{"steps":["feiten verzamelen","vergelijken"]}"#.into(),
            "feit A en feit B".into(),
            "A is sneller dan B".into(),
        ]));
        let ws = Workspace::new(RunId::new(), TraceId::new(), "vergelijk", None);
        let plan = make_plan(&llm, &ws, "vergelijk A en B", "").await;
        assert_eq!(plan.steps.len(), 2);
        let mut seen = Vec::new();
        let r = execute_plan(&llm, &ws, "vergelijk A en B", 4, |i, s, _| seen.push((i, s.to_string()))).await;
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].1, "A is sneller dan B");
        assert!(ws.snapshot().goals.iter().all(|g| g.status == GoalStatus::Done));
        assert_eq!(seen.len(), 2);
    }

    #[tokio::test]
    async fn bad_plan_json_falls_back_and_budget_limits() {
        let llm = Script(Mutex::new(vec!["geen json".into(), "x".into()]));
        let ws = Workspace::new(RunId::new(), TraceId::new(), "t", None);
        assert_eq!(make_plan(&llm, &ws, "doe iets", "").await.steps, vec!["doe iets".to_string()]);
        let ws2 = Workspace::new(RunId::new(), TraceId::new(), "t", None);
        for i in 0..5 {
            ws2.push_goal(&format!("s{i}"), None, vec![]);
        }
        let llm2 = Script(Mutex::new(vec!["r".into(); 5]));
        let r = execute_plan(&llm2, &ws2, "t", 2, |_, _, _| {}).await;
        assert_eq!(r.len(), 2);
        assert!(!ws2.snapshot().uncertainties.is_empty());
    }

    struct Calc;
    #[async_trait::async_trait]
    impl ToolRunner for Calc {
        fn tools(&self) -> Vec<vortex_models::backend::ToolSpec> {
            vec![vortex_models::backend::ToolSpec {
                name: "calc".into(),
                description: "rekenen".into(),
                parameters: serde_json::json!({}),
            }]
        }
        async fn run(&self, _n: &str, a: serde_json::Value) -> Result<String, String> {
            Ok(format!("{}", a["a"].as_i64().unwrap_or(0) * a["b"].as_i64().unwrap_or(0)))
        }
    }

    #[tokio::test]
    async fn react_loop_uses_tool_then_answers() {
        let llm = Script(Mutex::new(vec![
            r#"{"thought":"rekenen","action":"calc","args":{"a":17,"b":23}}"#.into(),
            r#"{"thought":"klaar","answer":"391"}"#.into(),
        ]));
        let mut obs = Vec::new();
        let a = react(&llm, &Calc, "17 × 23", 4, |_, o| obs.push(o.map(String::from))).await;
        assert_eq!(a.as_deref(), Ok("391"));
        assert_eq!(obs[0].as_deref(), Some("391"));
        let crit =
            critique(&Script(Mutex::new(vec![r#"{"verdict":"revise","issues":["rekenfout"]}"#.into()])), "t", "d")
                .await;
        assert_eq!(crit.issues, vec!["rekenfout".to_string()]);
    }

    #[tokio::test]
    async fn cse_incremental_and_voi() {
        assert_eq!(
            dedup_steps(vec![
                "Zoek de prijs van product A op".into(),
                "zoek de prijs van product A op.".into(),
                "Vergelijk A met B".into()
            ])
            .len(),
            2
        );
        assert!(low_value("Controleer de hoofdstad van Frankrijk", 0.9));
        assert!(!low_value("Bereken de som", 0.9));
        assert!(!low_value("Controleer de hoofdstad", 0.4));
        let cache = StepCache::default();
        // Eerste run: twee stappen → twee modelaanroepen.
        let ws = Workspace::new(RunId::new(), TraceId::new(), "t", None);
        ws.push_goal("stap een", None, vec![]);
        let llm = Script(Mutex::new(vec!["r1".into(), "r2".into()]));
        let first = execute_plan_with(&llm, &ws, "taak", 5, Some(&cache), 0.0, |_, _, _| {}).await;
        assert_eq!(first[0].1, "r1");
        // Tweede run met dezelfde stap en invoer: geen modelaanroep (script is leeg genoeg).
        let ws2 = Workspace::new(RunId::new(), TraceId::new(), "t", None);
        ws2.push_goal("stap een", None, vec![]);
        let empty = Script(Mutex::new(vec![]));
        let again = execute_plan_with(&empty, &ws2, "taak", 5, Some(&cache), 0.0, |_, _, _| {}).await;
        assert_eq!(again[0].1, "r1", "hergebruikt i.p.v. opnieuw berekend");
        // VOI: opzoekstap bij hoge geheugenzekerheid → overgeslagen zonder model.
        let ws3 = Workspace::new(RunId::new(), TraceId::new(), "t", None);
        ws3.push_goal("zoek de hoofdstad op", None, vec![]);
        let v = execute_plan_with(&empty, &ws3, "taak", 5, None, 0.95, |_, _, _| {}).await;
        assert!(v[0].1.starts_with("(overgeslagen"));
    }
}
