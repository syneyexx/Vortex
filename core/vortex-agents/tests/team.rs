//! W09-tests met een gescript nep-model: selectie op objectieve checks, Judge-gate met
//! herziening, offline modus, bewijsoverzicht, conflict-team (regel, periodes, betwist, rollback).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use vortex_agents::conflict::{ConflictDeps, resolve};
use vortex_agents::debate::{DebateDeps, run, synthesizer_guidance};
use vortex_agents::{Label, Reporter, Roles, TeamPlan};
use vortex_cognition::{CogLlm, GenOpts};
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog, ResourceRegistry, Supervisor};
use vortex_memory::{
    HashEmbedder, HeuristicLlm, MemConfig, MemoryKind, MemoryStore, NewMemory, Resolution, WriteOutcome,
};
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub, ToolHubConfig};
use vortex_types::{EventKind, Provenance, SourceKind, TraceId};

#[derive(Debug)]
struct Fake {
    judge: Mutex<Vec<&'static str>>,
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn new(judge: Vec<&'static str>) -> Arc<Self> {
        Arc::new(Self { judge: Mutex::new(judge), calls: Mutex::new(vec![]) })
    }
}

#[async_trait::async_trait]
impl CogLlm for Fake {
    async fn complete(&self, m: Vec<Message>, _o: GenOpts) -> Result<String, String> {
        let sys = m.first().map(|x| x.content.clone()).unwrap_or_default();
        let user = m.last().map(|x| x.content.clone()).unwrap_or_default();
        self.calls.lock().push(sys.chars().take(40).collect());
        Ok(if sys.contains("Judge van het conflict-team") {
            json!({"decision": "periods", "switch_date": "2025-01-01", "reason": "verhuisd"}).to_string()
        } else if sys.contains("onderzoeker in het conflict-team") {
            json!({"argument": "pleidooi", "evidence": []}).to_string()
        } else if sys.contains("bronbeoordelaar") {
            json!({"a_quality": 0.6, "b_quality": 0.6, "notes": ""}).to_string()
        } else if sys.contains("Je bent een Solver") {
            if user.contains("Feedback van Judge") {
                "Herzien: het antwoord is 395.".into()
            } else if sys.contains("eerste principes") {
                "Het antwoord is 400.".into()
            } else {
                "Vortex draait lokaal. Het antwoord is 395.".into()
            }
        } else if sys.contains("Je bent de Planner") {
            json!({"steps": ["som opzetten", "uitrekenen"]}).to_string()
        } else if sys.contains("Critic") {
            json!({"issues": [{"severity": "low", "text": "mag korter"}]}).to_string()
        } else if sys.contains("Splits de tekst in losse") {
            json!({"claims": ["Vortex draait lokaal"]}).to_string()
        } else if sys.contains("strenge fact-checker") {
            json!({"checks": [{"i": 0, "label": "supported", "why": "staat in het geheugen"}]}).to_string()
        } else if sys.contains("kort Python-programma") {
            json!({"code": "print(17*23+4)"}).to_string()
        } else if sys.contains("Je bent de Judge") {
            let v = {
                let mut j = self.judge.lock();
                if j.len() > 1 { j.remove(0) } else { j[0] }
            };
            match v {
                "revise" => {
                    json!({"verdict": "revise", "confidence": 0.4, "reasons": ["onzeker"], "fix": "controleer de som"})
                        .to_string()
                }
                "uncertain" => {
                    json!({"verdict": "uncertain", "confidence": 0.3, "reasons": ["te weinig bewijs"]}).to_string()
                }
                _ => json!({"verdict": "accept", "confidence": 0.9, "reasons": ["klopt met berekening"]}).to_string(),
            }
        } else {
            "{}".into()
        })
    }
}

fn log() -> EventLog {
    EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()))
}

fn store(l: &EventLog) -> Arc<MemoryStore> {
    Arc::new(
        MemoryStore::open(l.clone(), Arc::new(HashEmbedder::new(256)), Arc::new(HeuristicLlm), MemConfig::default())
            .unwrap(),
    )
}

fn have_python() -> bool {
    std::process::Command::new(vortex_tools::sandbox::default_python()).arg("--version").output().is_ok()
}

fn team(spec: Option<&str>, fact: bool, rounds: usize) -> TeamPlan {
    TeamPlan {
        planner: true,
        solvers: 2,
        fact_checker: fact,
        critic: true,
        specialist: spec.map(String::from),
        judge: true,
        max_rounds: rounds,
        parallel: true,
        why: vec![],
    }
}

#[tokio::test]
async fn objective_check_picks_correct_candidate_and_judge_accepts() {
    if !have_python() {
        return;
    }
    let l = log();
    let d = tempfile::tempdir().unwrap();
    let hub = ToolHub::new(
        ToolHubConfig { skills_dir: Some(d.path().join("s")), workdir: Some(d.path().join("w")), ..Default::default() },
        l.clone(),
        Supervisor::new(l.clone(), ResourceRegistry::new(l.clone())),
        Arc::new(|_| None),
    );
    let fake = Fake::new(vec!["accept"]);
    let run_id = vortex_types::RunId::new();
    let deps = DebateDeps {
        main: fake.clone(),
        aux: None,
        extra: vec![],
        memory: None,
        tools: Some(hub),
        online: false,
        roles: Roles::defaults(),
        reporter: Reporter::new(Some(l.clone()), Some(run_id), TraceId::new()),
        exec: ExecOptions { run: Some(run_id), ..Default::default() },
        workspace: None,
        material: vec![],
        scorer: None,
        calibrator: None,
    };
    let o = run(&deps, "Bereken 17 × 23 + 4", &team(Some("math"), false, 2)).await;
    assert!(o.trusted);
    assert!(o.draft.contains("395"), "kandidaat met geslaagde rekencheck wint: {}", o.draft);
    let chosen = o.candidates.iter().find(|c| c.3).unwrap();
    let other = o.candidates.iter().find(|c| !c.3).unwrap();
    assert!(chosen.2 > other.2, "{:?}", o.candidates);
    assert_eq!(o.plan.len(), 2);
    // Statusevents voor het Agent collaboration-paneel.
    let ev = l.read_from(0, 1000).unwrap();
    let roles: Vec<String> = ev
        .iter()
        .filter(|e| e.kind == EventKind::AgentRoleCompleted)
        .map(|e| e.payload["role"].as_str().unwrap_or("").to_string())
        .collect();
    for r in ["Planner", "Solvers", "Critic", "Specialist", "Judge"] {
        assert!(roles.iter().any(|x| x == r), "{r} ontbreekt in {roles:?}");
    }
    assert!(ev.iter().any(|e| e.kind == EventKind::TeamComposed && e.run_id == Some(run_id)));
    let g = synthesizer_guidance(&deps.roles, &o);
    assert!(g.contains("Goedgekeurd concept") && !g.contains("NIET volledig"));
}

#[tokio::test]
async fn judge_gate_revises_then_reports_uncertainty() {
    let l = log();
    // Revise → herziening → accept.
    let fake = Fake::new(vec!["revise", "accept"]);
    let deps = DebateDeps {
        main: fake.clone(),
        aux: None,
        extra: vec![],
        memory: None,
        tools: None,
        online: false,
        roles: Roles::defaults(),
        reporter: Reporter::new(Some(l.clone()), None, TraceId::new()),
        exec: ExecOptions::default(),
        workspace: None,
        material: vec![],
        scorer: None,
        calibrator: None,
    };
    let o = run(&deps, "Leg uit en vergelijk", &team(None, false, 3)).await;
    assert_eq!(o.rounds, 2);
    assert!(o.trusted);
    assert!(o.draft.starts_with("Herzien"), "{}", o.draft);
    // Altijd onzeker → na max rondes niet vertrouwd; Synthesizer moet eerlijk zijn.
    let deps2 = DebateDeps { main: Fake::new(vec!["revise"]), ..deps.clone() };
    let o2 = run(&deps2, "Leg uit en vergelijk", &team(None, false, 2)).await;
    assert!(!o2.trusted);
    assert_eq!(o2.rounds, 2);
    assert!(synthesizer_guidance(&deps.roles, &o2).contains("NIET volledig"));
}

#[tokio::test]
async fn offline_fact_check_is_local_and_labelled() {
    let l = log();
    let st = store(&l);
    let mut nm = NewMemory::new(
        MemoryKind::Project,
        "Vortex draait lokaal op de eigen PC van de gebruiker.",
        Provenance::new(SourceKind::User),
    );
    nm.force = true;
    st.write(nm).await.unwrap();
    let fake = Fake::new(vec!["accept"]);
    let deps = DebateDeps {
        main: fake.clone(),
        aux: None,
        extra: vec![],
        memory: Some((st, None)),
        tools: None,
        online: false,
        roles: Roles::defaults(),
        reporter: Reporter::new(Some(l.clone()), None, TraceId::new()),
        exec: ExecOptions::default(),
        workspace: None,
        material: vec![],
        scorer: None,
        calibrator: None,
    };
    let o = run(&deps, "Waar draait Vortex en waarom?", &team(None, true, 2)).await;
    assert_eq!(o.scope, "lokaal");
    assert!(o.evidence.iter().all(|c| c.scope == "lokaal"));
    assert_eq!(o.evidence[0].label, Label::Supported);
    assert_eq!(o.evidence[0].sources[0].kind, "geheugen");
    let ev = l.read_from(0, 1000).unwrap();
    assert!(!ev.iter().any(|e| e.kind == EventKind::ToolCallStarted), "offline: geen webcalls");
    let fc =
        ev.iter().find(|e| e.kind == EventKind::AgentRoleCompleted && e.payload["role"] == "Fact-checker").unwrap();
    assert!(fc.payload["summary"].as_str().unwrap().contains("lokaal geverifieerd"));
    assert!(ev.iter().any(|e| e.kind == EventKind::EvidenceCompiled));
    assert!(synthesizer_guidance(&deps.roles, &o).contains("alleen lokaal geverifieerd"));
}

async fn mem(st: &MemoryStore, text: &str, src: SourceKind, date: &str) -> vortex_types::MemoryId {
    let mut nm = NewMemory::new(MemoryKind::Semantic, text, Provenance::with_origin(src, "test"));
    nm.force = true;
    nm.event_time = Some(date.into());
    match st.write(nm).await.unwrap() {
        WriteOutcome::Stored { ids, .. } => ids[0],
        o => panic!("{o:?}"),
    }
}

fn conflict(l: &EventLog, a: vortex_types::MemoryId, b: vortex_types::MemoryId) -> String {
    let id = vortex_types::EventId::new().to_string();
    l.db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO conflicts(id, a_memory, b_memory, reason, status, created_at) VALUES (?1, ?2, ?3, 'test', 'open', '2026-01-01')",
                rusqlite::params![id, a.to_string(), b.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    id
}

#[tokio::test]
async fn conflict_team_rules_periods_disputed_and_rollback() {
    let l = log();
    let st = store(&l);
    let deps = ConflictDeps { llm: Fake::new(vec!["accept"]), roles: Roles::defaults(), reporter: Reporter::silent() };

    // 1. Persoonlijk: gebruiker wint van web.
    let a = mem(&st, "Syneye woont in Utrecht.", SourceKind::User, "2024-03-01").await;
    let b = mem(&st, "Syneye woont in Amsterdam volgens een webpagina.", SourceKind::Web, "2024-06-01").await;
    let c1 = conflict(&l, a, b);
    let c = st.conflicts(Some("open")).unwrap().into_iter().find(|c| c.id == c1).unwrap();
    let r = resolve(&deps, &st, &c).await.unwrap();
    assert!(matches!(r, Resolution::Winner { winner, .. } if winner == a));
    assert!(st.get(b).unwrap().valid_to.is_some(), "verliezer wordt historie");

    // 2. Team: tijdgebonden → periodes.
    let x = mem(&st, "Het kantoor zit aan de Oudegracht.", SourceKind::File, "2023-01-01").await;
    let y = mem(&st, "Het kantoor zit aan de Catharijnesingel.", SourceKind::File, "2025-02-01").await;
    let c2 = conflict(&l, x, y);
    let c = st.conflicts(Some("open")).unwrap().into_iter().find(|c| c.id == c2).unwrap();
    let r = resolve(&deps, &st, &c).await.unwrap();
    assert!(matches!(r, Resolution::Periods { .. }), "{r:?}");
    assert_eq!(st.get(x).unwrap().valid_to.as_deref(), Some("2025-01-01"));
    assert_eq!(st.get(y).unwrap().valid_from.as_deref(), Some("2025-01-01"));

    // 3. Rollback zet alles terug.
    assert_eq!(st.rollback_conflict(&c2).unwrap(), 2);
    assert_eq!(st.get(x).unwrap().valid_to, None);
    assert_eq!(st.conflicts(Some("open")).unwrap().iter().filter(|c| c.id == c2).count(), 1);

    // 4. Eén webpagina wint niet automatisch.
    #[derive(Debug)]
    struct PickWeb;
    #[async_trait::async_trait]
    impl CogLlm for PickWeb {
        async fn complete(&self, m: Vec<Message>, _o: GenOpts) -> Result<String, String> {
            let sys = &m[0].content;
            Ok(if sys.contains("Judge van het conflict-team") {
                json!({"decision": "b", "reason": "nieuwer"}).to_string()
            } else if sys.contains("bronbeoordelaar") {
                json!({"a_quality": 0.6, "b_quality": 0.65}).to_string()
            } else {
                "{}".into()
            })
        }
    }
    let p = mem(&st, "De bibliotheek sluit om 17:00.", SourceKind::Model, "2024-01-01").await;
    let q = mem(&st, "De bibliotheek sluit om 18:00.", SourceKind::Web, "2024-02-01").await;
    let c3 = conflict(&l, p, q);
    let c = st.conflicts(Some("open")).unwrap().into_iter().find(|c| c.id == c3).unwrap();
    let r = resolve(&ConflictDeps { llm: Arc::new(PickWeb), ..deps.clone() }, &st, &c).await.unwrap();
    assert!(matches!(r, Resolution::Disputed { .. }), "{r:?}");
    assert!(st.conflicts(Some("disputed")).unwrap().iter().any(|c| c.id == c3));
}
