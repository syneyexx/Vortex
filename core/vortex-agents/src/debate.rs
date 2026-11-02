//! Het debat (W09.03–W09.08, W09.11): Planner → Solvers (diversiteit, geïsoleerde contexten,
//! parallel) → per kandidaat Critic + Fact-checker + Specialist → verifier-guided selectie →
//! Judge-gate (rondes met herziening) → instructie voor de Synthesizer + bewijsoverzicht.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use vortex_cognition::{GenOpts, Workspace};
use vortex_context::{Budget, assemble_for_role};
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub};
use vortex_types::EventKind;

use crate::evidence::{ClaimCheck, summary};
use crate::factcheck::{FactDeps, check, extract_claims};
use crate::json::{extract, strings};
use crate::roles::Roles;
use crate::specialist::{SpecialistReport, issues, last_number};
use crate::team::TeamPlan;
use crate::{Llm, Reporter};

/// Alles wat het team nodig heeft.
#[derive(Debug, Clone)]
pub struct DebateDeps {
    pub main: Llm,
    /// 1060-model (claims splitsen); valt terug op `main`.
    pub aux: Option<Llm>,
    /// Extra modellen voor solver-diversiteit (optioneel).
    pub extra: Vec<Llm>,
    pub memory: Option<crate::MemoryAccess>,
    pub tools: Option<Arc<ToolHub>>,
    pub online: bool,
    pub roles: Roles,
    pub reporter: Reporter,
    pub exec: ExecOptions,
    pub workspace: Option<Workspace>,
    /// Context van de cyclus (herinneringen, sturing) als materiaal voor de rollen.
    pub material: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub angle: String,
    pub text: String,
    pub critic: Vec<(String, String)>,
    pub evidence: Vec<ClaimCheck>,
    pub specialist: Option<SpecialistReport>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub verdict: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebateOutcome {
    pub draft: String,
    pub trusted: bool,
    pub verdict: Verdict,
    pub evidence: Vec<ClaimCheck>,
    pub candidates: Vec<(String, String, f64, bool)>,
    pub rounds: usize,
    pub scope: String,
    pub plan: Vec<String>,
}

const ANGLES: &[&str] = &[
    "Direct en grondig: beantwoord de vraag stap voor stap.",
    "Vanuit eerste principes: leid het af, controleer elke stap en reken na.",
    "Vanuit voorbeelden en tegenvoorbeelden: toets je antwoord aan concrete gevallen en randgevallen.",
    "Als scepticus: wat zou er mis kunnen zijn met de voor de hand liggende conclusie?",
];

fn solver_llm(d: &DebateDeps, i: usize) -> Llm {
    // Diversiteit in model als er extra modellen zijn: solver 2, 3… roteren.
    if i > 0 && !d.extra.is_empty() { d.extra[(i - 1) % d.extra.len()].clone() } else { d.main.clone() }
}

async fn solve(d: &DebateDeps, i: usize, question: &str, plan: &[String], revise: Option<(&str, &str)>) -> Candidate {
    let r = d.roles.get("solver");
    let angle = ANGLES[i % ANGLES.len()];
    let temp = r.temperature.unwrap_or(0.7) * [0.6, 1.0, 1.3, 1.1][i % 4];
    let mut material = d.material.clone();
    if !plan.is_empty() {
        material.push(format!("Plan van de Planner:\n- {}", plan.join("\n- ")));
    }
    let task = match revise {
        Some((prev, fb)) => format!(
            "Vraag: {question}\n\nJe vorige antwoord:\n{prev}\n\nFeedback van Judge en Critic — herzie je antwoord hierop:\n{fb}"
        ),
        None => question.to_string(),
    };
    let a = assemble_for_role(&format!("{}\nInvalshoek: {angle}", r.prompt), &task, &material, &Budget::default());
    let text = solver_llm(d, i)
        .complete(
            a.messages,
            GenOpts {
                max_tokens: r.max_tokens.or(Some(1800)),
                temperature: Some(temp),
                json_schema: None,
                reasoning_effort: Some("medium".into()),
            },
        )
        .await
        .unwrap_or_default();
    Candidate {
        id: format!("kandidaat-{}", i + 1),
        angle: angle.into(),
        text,
        critic: vec![],
        evidence: vec![],
        specialist: None,
        score: 0.0,
    }
}

fn fact_deps(d: &DebateDeps) -> FactDeps {
    FactDeps {
        llm: d.main.clone(),
        aux: d.aux.clone(),
        memory: d.memory.clone(),
        tools: d.tools.clone(),
        online: d.online,
        exec: d.exec.clone(),
    }
}

/// Critic + fact-check + specialist voor één kandidaat, dan de verifier-score.
async fn review(d: &DebateDeps, team: &TeamPlan, question: &str, c: &mut Candidate) {
    if c.text.trim().is_empty() {
        c.score = -1.0;
        return;
    }
    if team.critic {
        let r = d.roles.get("critic");
        c.critic =
            issues(&d.main, &r.prompt, r.temperature, &format!("Vraag: {question}\n\nAntwoord:\n{}", c.text)).await;
    }
    if team.fact_checker {
        let fd = fact_deps(d);
        let claims = extract_claims(&fd, &d.roles, &c.text).await;
        c.evidence = check(&fd, &claims).await;
    }
    let key = format!("{}:{}", d.exec.run.map(|r| r.to_string()).unwrap_or_default(), c.id);
    c.specialist = match team.specialist.as_deref() {
        Some("code") => {
            Some(crate::specialist::code(&d.main, &d.roles, d.tools.as_ref(), &d.exec, &key, &c.text).await)
        }
        Some("math") => {
            Some(crate::specialist::math(&d.main, d.tools.as_ref(), &d.exec, &key, question, &c.text).await)
        }
        _ => None,
    };
    c.score = verifier_score(c);
}

/// Verifier-guided selectie: objectieve checks wegen het zwaarst.
pub fn verifier_score(c: &Candidate) -> f64 {
    let mut s = 0.5;
    s += 0.3 * ClaimCheck::score(&c.evidence);
    if let Some(sp) = &c.specialist {
        match sp.passed {
            Some(true) => s += 0.3,
            Some(false) => s -= 0.4,
            None => {}
        }
        for (sev, _) in &sp.issues {
            s -= if sev == "high" { 0.1 } else { 0.0 };
        }
    }
    let pen: f64 = c
        .critic
        .iter()
        .map(|(sev, _)| match sev.as_str() {
            "high" => 0.15,
            "medium" => 0.05,
            _ => 0.0,
        })
        .sum();
    s - pen.min(0.45)
}

async fn judge(d: &DebateDeps, question: &str, best: &Candidate, others: &[Candidate]) -> Verdict {
    let r = d.roles.get("judge");
    let mut u = format!(
        "Vraag: {question}\n\nBeste kandidaat ({}, verifier-score {:.2}):\n{}\n",
        best.id, best.score, best.text
    );
    if !best.critic.is_empty() {
        u.push_str("\nCritic:\n");
        for (s, t) in &best.critic {
            u.push_str(&format!("- [{s}] {t}\n"));
        }
    }
    if !best.evidence.is_empty() {
        u.push_str(&format!("\nFact-check ({}):\n{}\n", best.evidence[0].scope, summary(&best.evidence)));
    }
    if let Some(sp) = &best.specialist
        && sp.ran
    {
        u.push_str(&format!(
            "\nObjectieve check ({}): {}\n{}\n",
            sp.kind,
            match sp.passed {
                Some(true) => "GESLAAGD",
                Some(false) => "MISLUKT",
                None => "geen uitkomst",
            },
            sp.output.chars().take(600).collect::<String>()
        ));
    }
    if !others.is_empty() {
        u.push_str("\nAndere kandidaten (kort):\n");
        for o in others {
            u.push_str(&format!(
                "- {} (score {:.2}): {}\n",
                o.id,
                o.score,
                o.text.chars().take(300).collect::<String>().replace('\n', " ")
            ));
        }
    }
    let schema = json!({"type": "object", "required": ["verdict", "confidence"], "properties": {
        "verdict": {"type": "string", "enum": ["accept", "revise", "uncertain"]},
        "confidence": {"type": "number"}, "reasons": {"type": "array", "items": {"type": "string"}}, "fix": {"type": "string"}}});
    let out = d
        .main
        .complete(
            vec![Message::system(r.prompt.clone()), Message::user(u)],
            GenOpts {
                max_tokens: Some(700),
                temperature: r.temperature,
                json_schema: Some(schema),
                reasoning_effort: Some("high".into()),
            },
        )
        .await
        .unwrap_or_default();
    let v = extract(&out).unwrap_or_default();
    Verdict {
        verdict: v["verdict"].as_str().unwrap_or("uncertain").to_string(),
        confidence: v["confidence"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
        reasons: strings(&v, "reasons"),
        fix: v["fix"].as_str().unwrap_or("").to_string(),
    }
}

/// Het hele debat.
pub async fn run(d: &DebateDeps, question: &str, team: &TeamPlan) -> DebateOutcome {
    let rep = &d.reporter;
    rep.emit(EventKind::TeamComposed, json!({"roles": team.roles(), "plan": team}));
    // 1. Planner.
    let mut plan = Vec::new();
    if team.planner {
        rep.started("Planner", "deelvragen bepalen");
        let r = d.roles.get("planner");
        let schema = json!({"type": "object", "required": ["steps"], "properties": {"steps": {"type": "array", "items": {"type": "string"}, "maxItems": 5}}});
        let a = assemble_for_role(&r.prompt, question, &d.material, &Budget::default());
        let out = d
            .main
            .complete(
                a.messages,
                GenOpts {
                    max_tokens: Some(500),
                    temperature: r.temperature,
                    json_schema: Some(schema),
                    reasoning_effort: None,
                },
            )
            .await
            .unwrap_or_default();
        plan = extract(&out).map(|v| strings(&v, "steps")).unwrap_or_default();
        if let Some(ws) = &d.workspace {
            for s in &plan {
                ws.push_goal(s, None, vec![]);
            }
        }
        rep.done("Planner", &format!("{} stappen", plan.len()), json!({"steps": plan}));
    }
    // 2. Solvers (parallel of na elkaar), geïsoleerde contexten.
    rep.started("Solvers", &format!("{} onafhankelijke uitwerkingen", team.solvers));
    let mut cands: Vec<Candidate> = if team.parallel {
        futures::future::join_all((0..team.solvers).map(|i| solve(d, i, question, &plan, None))).await
    } else {
        let mut v = Vec::new();
        for i in 0..team.solvers {
            v.push(solve(d, i, question, &plan, None).await);
        }
        v
    };
    if let Some(ws) = &d.workspace {
        for c in &cands {
            ws.add_hypothesis(&format!("{}: {}", c.id, c.text.chars().take(200).collect::<String>()), 0.5);
        }
    }
    rep.done(
        "Solvers",
        &format!("{} kandidaten", cands.iter().filter(|c| !c.text.trim().is_empty()).count()),
        json!({"angles": cands.iter().map(|c| c.angle.clone()).collect::<Vec<_>>()}),
    );

    let mut rounds = 0;
    let mut verdict;
    loop {
        rounds += 1;
        // 3. Review per kandidaat (Critic + Fact-checker + Specialist) → score.
        rep.started("Critic", &format!("ronde {rounds}: kandidaten nalopen"));
        for c in cands.iter_mut().filter(|c| c.score == 0.0) {
            review(d, team, question, c).await;
        }
        cands.sort_by(|a, b| b.score.total_cmp(&a.score));
        let best = cands[0].clone();
        let high = best.critic.iter().filter(|(s, _)| s == "high").count();
        rep.done(
            "Critic",
            &format!("{} punten bij de beste ({} ernstig)", best.critic.len(), high),
            json!({"issues": best.critic}),
        );
        if team.fact_checker {
            crate::factcheck::report(rep, &best.evidence);
        }
        if let Some(sp) = &best.specialist {
            rep.done(
                "Specialist",
                &match sp.passed {
                    Some(true) => format!("{}: check geslaagd", sp.kind),
                    Some(false) => format!("{}: check MISLUKT", sp.kind),
                    None => format!("{}: geen objectieve check mogelijk", sp.kind),
                },
                json!({"kind": sp.kind, "ran": sp.ran, "passed": sp.passed}),
            );
        }
        // 4. Judge-gate.
        rep.started("Judge", &format!("ronde {rounds}"));
        verdict = judge(d, question, &best, &cands[1..]).await;
        rep.emit(
            EventKind::JudgeVerdict,
            json!({"round": rounds, "verdict": verdict, "candidate": best.id, "score": best.score}),
        );
        rep.done(
            "Judge",
            &format!("{} (vertrouwen {:.0}%)", verdict.verdict, verdict.confidence * 100.0),
            json!({"verdict": verdict}),
        );
        let accept = verdict.verdict == "accept" && verdict.confidence >= 0.6;
        if accept || rounds >= team.max_rounds || verdict.verdict == "uncertain" {
            break;
        }
        // 5. Herzien: de beste kandidaat krijgt de feedback.
        rep.started("Solvers", "herzien op feedback van de Judge");
        let mut fb = verdict.fix.clone();
        for (s, t) in &best.critic {
            fb.push_str(&format!("\n- [{s}] {t}"));
        }
        for c in best.evidence.iter().filter(|c| c.label == crate::Label::Contradicted) {
            fb.push_str(&format!("\n- tegengesproken: {} ({})", c.claim, c.why));
        }
        let idx: usize = best.id.trim_start_matches("kandidaat-").parse::<usize>().unwrap_or(1).saturating_sub(1);
        let mut nc = solve(d, idx, question, &plan, Some((&best.text, &fb))).await;
        nc.id = format!("{}-r{rounds}", best.id);
        cands.insert(0, nc);
        rep.done("Solvers", "herziene kandidaat klaar", json!({}));
    }
    let best = cands[0].clone();
    let trusted = verdict.verdict == "accept" && verdict.confidence >= 0.6;
    let scope = best
        .evidence
        .first()
        .map(|c| c.scope.clone())
        .unwrap_or_else(|| if d.online { "lokaal+web".into() } else { "lokaal".into() });
    rep.emit(
        EventKind::EvidenceCompiled,
        json!({"checks": best.evidence, "scope": scope, "specialist": best.specialist}),
    );
    if let Some(ws) = &d.workspace {
        ws.add_note(&format!("judge: {} ({:.2}) na {rounds} ronde(s)", verdict.verdict, verdict.confidence));
        if !trusted {
            ws.add_uncertainty(&format!("Judge vertrouwt het antwoord niet: {}", verdict.reasons.join("; ")));
        }
    }
    DebateOutcome {
        draft: best.text.clone(),
        trusted,
        verdict,
        evidence: best.evidence.clone(),
        candidates: cands.iter().map(|c| (c.id.clone(), c.angle.clone(), c.score, c.id == best.id)).collect(),
        rounds,
        scope,
        plan,
    }
}

/// Instructie voor de Synthesizer (het gestreamde eindantwoord).
pub fn synthesizer_guidance(roles: &Roles, o: &DebateOutcome) -> String {
    let r = roles.get("synthesizer");
    let mut g = format!(
        "{}\n\nGoedgekeurd concept van het team (Judge: {}, vertrouwen {:.0}%, {} ronde(s)):\n{}\n",
        r.prompt,
        o.verdict.verdict,
        o.verdict.confidence * 100.0,
        o.rounds,
        o.draft
    );
    let bad: Vec<&ClaimCheck> = o.evidence.iter().filter(|c| c.label == crate::Label::Contradicted).collect();
    if !bad.is_empty() {
        g.push_str("\nDeze beweringen zijn TEGENGESPROKEN door bewijs — neem ze niet over:\n");
        for c in bad {
            g.push_str(&format!("- {} ({})\n", c.claim, c.why));
        }
    }
    if !o.trusted {
        g.push_str("\nDe Judge vertrouwt het antwoord NIET volledig. Zeg eerlijk wat onzeker is");
        if !o.verdict.reasons.is_empty() {
            g.push_str(&format!(" ({})", o.verdict.reasons.join("; ")));
        }
        g.push_str(", geef alleen wat wél onderbouwd is, en stel zo nodig één gerichte vraag aan de gebruiker.\n");
    }
    if o.scope == "lokaal" && !o.evidence.is_empty() {
        g.push_str("\nFeiten zijn alleen lokaal geverifieerd (geen internetcheck); vermeld dat kort als het om actuele feiten gaat.\n");
    }
    g
}

/// Rekenkundige overeenstemming tussen kandidaten (alleen ter info voor de Judge).
pub fn numeric_agreement(cands: &[Candidate]) -> usize {
    let nums: Vec<f64> = cands.iter().filter_map(|c| last_number(&c.text)).collect();
    nums.iter().map(|a| nums.iter().filter(|b| (*a - **b).abs() < 1e-9).count()).max().unwrap_or(0)
}
