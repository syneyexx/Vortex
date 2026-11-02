//! Eval-runner: draait een suite tegen een `Subject`, scoort automatisch en bewaart alles per
//! versie in de database. Vergelijken met de vorige run = één aanroep (W06-gate).
// VX:W06.01

use std::time::Instant;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventId, EventKind, TraceId};

use crate::checks::run_check;
use crate::model::{EvalTask, Suite};

/// Het systeem dat getest wordt (Vortex zelf; geïmplementeerd in de server).
#[async_trait::async_trait]
pub trait Subject: Send + Sync {
    /// Versie-identiteit: config-versie + build + model.
    fn version(&self) -> String;
    /// Welke functies er al zijn (bv. "W08" als tools bestaan).
    fn has(&self, feature: &str) -> bool;
    /// Vooraf te onthouden feiten in een geïsoleerde eval-werkruimte.
    async fn prepare(&self, workspace: &str, facts: &[String]) -> Result<(), String>;
    /// Beantwoord de vraag (met geheugen uit `workspace`), zonder iets blijvends op te slaan.
    async fn answer(&self, workspace: &str, prompt: &str, mode: Option<&str>) -> Result<String, String>;
    /// Aantal open conflicten in de werkruimte (voor conflict-taken).
    async fn open_conflicts(&self, workspace: &str) -> usize;
    /// Eval-werkruimte opruimen (geen lekkage naar het echte geheugen).
    async fn cleanup(&self, workspace: &str);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub status: String,
    pub score: f64,
    pub latency_ms: u64,
    pub output: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub suite: String,
    pub version: String,
    pub label: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub n: i64,
    pub passed: i64,
    pub skipped: i64,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub suite: String,
    pub current: RunSummary,
    pub previous: Option<RunSummary>,
    pub delta: Option<f64>,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
    pub verdict: String,
}

pub struct Runner<'a> {
    pub log: &'a EventLog,
    pub subject: &'a dyn Subject,
    pub python: String,
}

impl std::fmt::Debug for Runner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner").finish()
    }
}

fn now() -> String {
    vortex_types::now().to_rfc3339()
}

impl Runner<'_> {
    async fn run_task(&self, suite: &str, t: &EvalTask) -> TaskResult {
        if let Some(req) = &t.requires
            && !self.subject.has(req)
        {
            return TaskResult {
                task_id: t.id.clone(),
                status: "skip".into(),
                score: 0.0,
                latency_ms: 0,
                output: String::new(),
                detail: format!("vereist {req}"),
            };
        }
        let ws = format!("__eval_{suite}_{}", t.id);
        if let Err(e) = self.subject.prepare(&ws, &t.memory).await {
            self.subject.cleanup(&ws).await;
            return TaskResult {
                task_id: t.id.clone(),
                status: "error".into(),
                score: 0.0,
                latency_ms: 0,
                output: String::new(),
                detail: e,
            };
        }
        let start = Instant::now();
        let ans = self.subject.answer(&ws, &t.prompt, t.mode.as_deref()).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let r = match ans {
            Err(e) => TaskResult {
                task_id: t.id.clone(),
                status: "error".into(),
                score: 0.0,
                latency_ms,
                output: String::new(),
                detail: e,
            },
            Ok(out) => {
                let conflicts = self.subject.open_conflicts(&ws).await;
                let mut passed = 0;
                let mut details = Vec::new();
                for c in &t.checks {
                    let r = run_check(c, &out, &self.python, conflicts).await;
                    passed += r.passed as usize;
                    details.push(format!("{}{}", if r.passed { "✓ " } else { "✗ " }, r.detail));
                }
                let score = if t.checks.is_empty() { 0.0 } else { passed as f64 / t.checks.len() as f64 };
                TaskResult {
                    task_id: t.id.clone(),
                    status: if score >= 1.0 { "pass".into() } else { "fail".into() },
                    score,
                    latency_ms,
                    output: out.chars().take(4000).collect(),
                    detail: details.join(" | "),
                }
            }
        };
        self.subject.cleanup(&ws).await;
        r
    }

    /// Draai een hele suite; resultaten + events in de database.
    pub async fn run_suite(&self, suite: &Suite, label: Option<&str>) -> Result<RunSummary, String> {
        let id = format!("evr_{}", EventId::new().to_string().trim_start_matches("evt_"));
        let version = self.subject.version();
        let trace = TraceId::new();
        let db = self.log.db();
        db.write(|tx| {
            tx.execute(
                "INSERT INTO eval_runs(id, suite, version, label, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, suite.name, version, label, now()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        let _ = self.log.append(Event::new(
            EventKind::EvalRunStarted,
            trace,
            serde_json::json!({"run": id, "suite": suite.name, "version": version}),
        ));
        let (mut passed, mut skipped, mut total) = (0i64, 0i64, 0f64);
        let mut lat = Vec::new();
        for t in &suite.tasks {
            let r = self.run_task(&suite.name, t).await;
            match r.status.as_str() {
                "pass" => passed += 1,
                "skip" => skipped += 1,
                _ => {}
            }
            if r.status != "skip" {
                total += r.score;
                lat.push(r.latency_ms);
            }
            db.write(|tx| {
                tx.execute(
                    "INSERT INTO eval_results(run_id, task_id, status, score, latency_ms, output, detail) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![id, r.task_id, r.status, r.score, r.latency_ms as i64, r.output, r.detail],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            let _ = self.log.append(Event::new(
                EventKind::EvalTaskScored,
                trace,
                serde_json::json!({"run": id, "task": r.task_id, "status": r.status, "score": r.score, "latency_ms": r.latency_ms}),
            ));
        }
        let scored = suite.tasks.len() as i64 - skipped;
        let score = (scored > 0).then(|| total / scored as f64);
        lat.sort_unstable();
        let p50 = lat.get(lat.len() / 2).copied();
        db.write(|tx| {
            tx.execute(
                "UPDATE eval_runs SET finished_at = ?2, n = ?3, passed = ?4, skipped = ?5, score = ?6, metrics = ?7 WHERE id = ?1",
                params![id, now(), suite.tasks.len() as i64, passed, skipped, score, serde_json::json!({"latency_p50_ms": p50}).to_string()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        let _ = self.log.append(Event::new(
            EventKind::EvalRunCompleted,
            trace,
            serde_json::json!({"run": id, "suite": suite.name, "score": score, "passed": passed, "skipped": skipped}),
        ));
        get_run(self.log, &id)?.ok_or_else(|| "run verdwenen".into())
    }
}

fn row_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummary> {
    Ok(RunSummary {
        id: r.get(0)?,
        suite: r.get(1)?,
        version: r.get(2)?,
        label: r.get(3)?,
        started_at: r.get(4)?,
        finished_at: r.get(5)?,
        n: r.get(6)?,
        passed: r.get(7)?,
        skipped: r.get(8)?,
        score: r.get(9)?,
    })
}

const SEL: &str = "SELECT id, suite, version, label, started_at, finished_at, n, passed, skipped, score FROM eval_runs";

pub fn get_run(log: &EventLog, id: &str) -> Result<Option<RunSummary>, String> {
    log.db()
        .read(|c| Ok(c.query_row(&format!("{SEL} WHERE id = ?1"), [id], row_run).optional()?))
        .map_err(|e| e.to_string())
}

/// Alle runs (nieuwste eerst), optioneel per suite.
pub fn list_runs(log: &EventLog, suite: Option<&str>, limit: usize) -> Result<Vec<RunSummary>, String> {
    log.db()
        .read(|c| {
            let mut st =
                c.prepare(&format!("{SEL} WHERE (?1 IS NULL OR suite = ?1) ORDER BY started_at DESC LIMIT ?2"))?;
            let rows = st.query_map(params![suite, limit as i64], row_run)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .map_err(|e| e.to_string())
}

pub fn results(log: &EventLog, run: &str) -> Result<Vec<TaskResult>, String> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT task_id, status, score, latency_ms, output, detail FROM eval_results WHERE run_id = ?1 ORDER BY task_id")?;
            let rows = st.query_map([run], |r| {
                Ok(TaskResult {
                    task_id: r.get(0)?,
                    status: r.get(1)?,
                    score: r.get(2)?,
                    latency_ms: r.get::<_, Option<i64>>(3)?.unwrap_or(0) as u64,
                    output: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    detail: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .map_err(|e| e.to_string())
}

/// Vergelijk een run met de vorige run van dezelfde suite. Verlies > 2 punten = "rood".
pub fn compare(log: &EventLog, run_id: &str) -> Result<Comparison, String> {
    let cur = get_run(log, run_id)?.ok_or("run onbekend")?;
    let prev =
        list_runs(log, Some(&cur.suite), 50)?.into_iter().find(|r| r.started_at < cur.started_at && r.id != cur.id);
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    if let Some(p) = &prev {
        let a: std::collections::HashMap<String, f64> =
            results(log, &p.id)?.into_iter().map(|r| (r.task_id, r.score)).collect();
        for r in results(log, run_id)? {
            if let Some(old) = a.get(&r.task_id) {
                if r.score + 1e-9 < *old {
                    regressions.push(r.task_id.clone());
                } else if r.score > old + 1e-9 {
                    improvements.push(r.task_id.clone());
                }
            }
        }
    }
    let delta = match (cur.score, prev.as_ref().and_then(|p| p.score)) {
        (Some(a), Some(b)) => Some(a - b),
        _ => None,
    };
    let verdict = match delta {
        None => "eerste meting".to_string(),
        Some(d) if d < -0.02 => format!("ROOD: {:.1} punt slechter", -d * 100.0),
        Some(d) if d > 0.02 => format!("GROEN: {:.1} punt beter", d * 100.0),
        Some(_) => "gelijk (binnen 2 punten)".to_string(),
    };
    Ok(Comparison { suite: cur.suite.clone(), current: cur, previous: prev, delta, regressions, improvements, verdict })
}
