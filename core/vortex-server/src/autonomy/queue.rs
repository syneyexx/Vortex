//! Taakqueue (W15.03): prioriteit, afhankelijkheden, niet-eerder-dan, herhaalpogingen met
//! backoff, pauzeren/hervatten, projecten (long-running, notities over sessies heen) en
//! herstel na een herstart (running → queued). Triggers (W15.04): cron, interval, event.
// VX:W15.03

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vortex_core::EventLog;
use vortex_types::{Event, EventId, EventKind, TraceId};

use super::cron::Cron;

fn now() -> String {
    vortex_types::now().to_rfc3339()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", EventId::new().to_string().trim_start_matches("evt_"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub params: Value,
    pub priority: i64,
    pub status: String,
    pub depends_on: Vec<String>,
    pub project_id: Option<String>,
    pub needs_idle: bool,
    pub not_before: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub progress: f64,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub run_id: Option<String>,
    pub origin: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewTask {
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub params: Value,
    /// 1 = hoog … 3 = achtergrond (P1–P3).
    #[serde(default = "three")]
    pub priority: i64,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Alleen uitvoeren als de gebruiker niet actief is (standaard bij P3).
    #[serde(default)]
    pub needs_idle: Option<bool>,
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<i64>,
    #[serde(default)]
    pub origin: Option<String>,
}
fn three() -> i64 {
    3
}

const COLS: &str = "id, title, kind, params, priority, status, depends_on, project_id, needs_idle, not_before, attempts, max_attempts, progress, result, error, run_id, origin, created_at, updated_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        params: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
        priority: r.get(4)?,
        status: r.get(5)?,
        depends_on: serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default(),
        project_id: r.get(7)?,
        needs_idle: r.get::<_, i64>(8)? == 1,
        not_before: r.get(9)?,
        attempts: r.get(10)?,
        max_attempts: r.get(11)?,
        progress: r.get(12)?,
        result: r.get::<_, Option<String>>(13)?.and_then(|s| serde_json::from_str(&s).ok()),
        error: r.get(14)?,
        run_id: r.get(15)?,
        origin: r.get(16)?,
        created_at: r.get(17)?,
        updated_at: r.get(18)?,
    })
}

#[derive(Debug, Clone)]
pub struct Queue {
    log: EventLog,
}

impl Queue {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    fn emit(&self, t: &Task) {
        let _ = self.log.append(Event::new(
            EventKind::TaskUpdated,
            TraceId::new(),
            json!({"id": t.id, "title": t.title, "kind": t.kind, "status": t.status, "progress": t.progress, "error": t.error, "project_id": t.project_id}),
        ));
    }

    pub fn enqueue(&self, n: NewTask) -> Result<Task, String> {
        if n.title.trim().is_empty() || n.kind.trim().is_empty() {
            return Err("titel en soort zijn verplicht".into());
        }
        for d in &n.depends_on {
            if self.get(d).is_none() {
                return Err(format!("onbekende afhankelijkheid {d}"));
            }
        }
        let id = new_id("task");
        let ts = now();
        let prio = n.priority.clamp(1, 3);
        let needs_idle = n.needs_idle.unwrap_or(prio == 3);
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "INSERT INTO tasks(id, title, kind, params, priority, status, depends_on, project_id, needs_idle, not_before, max_attempts, origin, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,'queued',?6,?7,?8,?9,?10,?11,?12,?12)",
                    params![
                        id,
                        n.title.trim(),
                        n.kind,
                        if n.params.is_null() { "{}".to_string() } else { n.params.to_string() },
                        prio,
                        serde_json::to_string(&n.depends_on).unwrap_or_default(),
                        n.project_id,
                        i64::from(needs_idle),
                        n.not_before,
                        n.max_attempts.unwrap_or(3).max(1),
                        n.origin.clone().unwrap_or_else(|| "user".into()),
                        ts
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        let t = self.get(&id).ok_or("taak niet opgeslagen")?;
        self.emit(&t);
        Ok(t)
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.log
            .db()
            .read(|c| Ok(c.query_row(&format!("SELECT {COLS} FROM tasks WHERE id = ?1"), [id], row).optional()?))
            .ok()
            .flatten()
    }

    pub fn list(&self, status: Option<&str>, project: Option<&str>, limit: usize) -> Vec<Task> {
        self.log
            .db()
            .read(|c| {
                let mut st = c.prepare(&format!(
                    "SELECT {COLS} FROM tasks WHERE (?1 IS NULL OR status = ?1) AND (?2 IS NULL OR project_id = ?2)
                     ORDER BY CASE status WHEN 'running' THEN 0 WHEN 'queued' THEN 1 WHEN 'paused' THEN 2 ELSE 3 END, priority, updated_at DESC LIMIT ?3"
                ))?;
                let v = st.query_map(params![status, project, limit as i64], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default()
    }

    fn set(&self, id: &str, sets: &str, p: &[&dyn rusqlite::ToSql]) {
        let sql = format!("UPDATE tasks SET {sets}, updated_at = '{}' WHERE id = '{}'", now(), id.replace('\'', ""));
        let _ = self.log.db().write(|tx| {
            tx.execute(&sql, p)?;
            Ok(())
        });
        if let Some(t) = self.get(id) {
            self.emit(&t);
        }
    }

    /// Volgende uitvoerbare taak: wachtrij, `not_before` verstreken, afhankelijkheden klaar,
    /// idle-eis gerespecteerd; laagste prioriteitsgetal eerst, dan oudste. Afhankelijkheid
    /// mislukt/geannuleerd → deze taak mislukt ook.
    pub fn next_runnable(&self, user_idle: bool, skip: &[String]) -> Option<Task> {
        let ts = now();
        for t in self.list(Some("queued"), None, 500).into_iter().rev().collect::<Vec<_>>().into_iter().rev() {
            if skip.contains(&t.id)
                || t.not_before.as_deref().is_some_and(|nb| nb > ts.as_str())
                || (t.needs_idle && !user_idle)
            {
                continue;
            }
            let deps: Vec<Option<Task>> = t.depends_on.iter().map(|d| self.get(d)).collect();
            if let Some(bad) = deps.iter().flatten().find(|d| d.status == "failed" || d.status == "cancelled") {
                self.set(
                    &t.id,
                    "status = 'failed', error = ?1",
                    &[&format!("afhankelijkheid {} {}", bad.id, bad.status)],
                );
                continue;
            }
            if deps.iter().all(|d| d.as_ref().is_some_and(|d| d.status == "done")) {
                return Some(t);
            }
        }
        None
    }

    pub fn start(&self, id: &str, run_id: Option<String>) {
        self.set(id, "status = 'running', attempts = attempts + 1, run_id = ?1, error = NULL", &[&run_id]);
    }

    pub fn progress(&self, id: &str, p: f64) {
        self.set(id, "progress = ?1", &[&p.clamp(0.0, 1.0)]);
    }

    /// Klaar of mislukt. Mislukt met pogingen over → opnieuw in de wachtrij met backoff.
    pub fn finish(&self, id: &str, r: Result<Value, String>) {
        let Some(t) = self.get(id) else { return };
        if t.status != "running" {
            return; // intussen geannuleerd/gepauzeerd
        }
        match r {
            Ok(v) => self.set(id, "status = 'done', progress = 1.0, result = ?1", &[&v.to_string()]),
            Err(e) if t.attempts < t.max_attempts => {
                let nb = (vortex_types::now() + chrono::Duration::minutes(2i64.pow(t.attempts as u32))).to_rfc3339();
                self.set(id, "status = 'queued', error = ?1, not_before = ?2", &[&e, &nb]);
            }
            Err(e) => self.set(id, "status = 'failed', error = ?1", &[&e]),
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let ok = self.get(id).is_some_and(|t| ["queued", "running", "paused"].contains(&t.status.as_str()));
        if ok {
            self.set(id, "status = 'cancelled'", &[]);
        }
        ok
    }

    pub fn pause(&self, id: &str) -> bool {
        let ok = self.get(id).is_some_and(|t| t.status == "queued" || t.status == "running");
        if ok {
            self.set(id, "status = 'paused'", &[]);
        }
        ok
    }

    pub fn resume(&self, id: &str) -> bool {
        let ok = self.get(id).is_some_and(|t| t.status == "paused" || t.status == "failed");
        if ok {
            self.set(
                id,
                "status = 'queued', not_before = NULL, attempts = CASE WHEN status = 'failed' THEN 0 ELSE attempts END",
                &[],
            );
        }
        ok
    }

    /// Na een herstart: wat liep, gaat terug in de wachtrij (hervatbaar).
    pub fn recover(&self) -> usize {
        let ids: Vec<String> = self.list(Some("running"), None, 1000).into_iter().map(|t| t.id).collect();
        for id in &ids {
            self.set(id, "status = 'queued', attempts = MAX(0, attempts - 1)", &[]);
        }
        ids.len()
    }

    // ── projecten ────────────────────────────────────────────────────────────

    pub fn create_project(&self, name: &str, goal: &str, workspace: Option<&str>) -> Result<Value, String> {
        if name.trim().is_empty() {
            return Err("naam is verplicht".into());
        }
        let id = new_id("proj");
        let ts = now();
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "INSERT INTO projects(id, name, goal, status, workspace, created_at, updated_at) VALUES (?1,?2,?3,'active',?4,?5,?5)",
                    params![id, name.trim(), goal, workspace, ts],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.project(&id).ok_or_else(|| "project niet opgeslagen".into())
    }

    pub fn project(&self, id: &str) -> Option<Value> {
        let p = self
            .log
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT id, name, goal, status, workspace, notes, created_at, updated_at FROM projects WHERE id = ?1", [id], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "goal": r.get::<_, String>(2)?, "status": r.get::<_, String>(3)?, "workspace": r.get::<_, Option<String>>(4)?, "notes": r.get::<_, String>(5)?, "created_at": r.get::<_, String>(6)?, "updated_at": r.get::<_, String>(7)?}))
                })
                .optional()?)
            })
            .ok()
            .flatten()?;
        let tasks = self.list(None, Some(id), 500);
        let done = tasks.iter().filter(|t| t.status == "done").count();
        let mut p = p;
        p["progress"] = json!(if tasks.is_empty() { 0.0 } else { done as f64 / tasks.len() as f64 });
        p["tasks"] = json!(tasks);
        Some(p)
    }

    pub fn projects(&self) -> Vec<Value> {
        let ids: Vec<String> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id FROM projects ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 ELSE 2 END, updated_at DESC")?;
                let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        ids.iter().filter_map(|i| self.project(i)).collect()
    }

    /// Notitie toevoegen (continuïteit over sessies: wat is gedaan, wat is de volgende stap).
    pub fn note(&self, project: &str, text: &str) {
        let line = format!("- {} {}\n", &now()[..16], text.trim());
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "UPDATE projects SET notes = notes || ?2, updated_at = ?3 WHERE id = ?1",
                params![project, line, now()],
            )?;
            Ok(())
        });
    }

    pub fn set_project_status(&self, project: &str, status: &str) -> Result<(), String> {
        if !["active", "paused", "done"].contains(&status) {
            return Err("status = active | paused | done".into());
        }
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "UPDATE projects SET status = ?2, updated_at = ?3 WHERE id = ?1",
                    params![project, status, now()],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

// ── triggers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub spec: String,
    pub task: NewTask,
    pub enabled: bool,
    pub last_fired: Option<String>,
    pub fired: i64,
    pub builtin: bool,
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Triggers {
    log: EventLog,
}

impl Triggers {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    pub fn add(&self, name: &str, kind: &str, spec: &str, task: &NewTask, builtin: bool) -> Result<String, String> {
        match kind {
            "cron" => {
                Cron::parse(spec)?;
            }
            "interval" => {
                spec.parse::<f64>()
                    .map_err(|_| "interval = minuten".to_string())
                    .and_then(|m| if m >= 1.0 { Ok(()) } else { Err("interval ≥ 1 minuut".into()) })?;
            }
            "event" => {
                if spec.trim().is_empty() {
                    return Err("event-patroon verplicht (bv. world.event.updated of memory.conflict.*)".into());
                }
            }
            k => return Err(format!("onbekende triggersoort {k}")),
        }
        let id = if builtin { format!("trg_{}", name.replace(' ', "_")) } else { new_id("trg") };
        // Event-triggers beginnen bij het huidige einde van het log (geen historie naspelen).
        let seq: i64 = self
            .log
            .db()
            .read(|c| Ok(c.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?))
            .unwrap_or(0);
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "INSERT OR IGNORE INTO triggers(id, name, kind, spec, task, enabled, last_seq, builtin, created_at) VALUES (?1,?2,?3,?4,?5,1,?6,?7,?8)",
                    params![id, name, kind, spec, serde_json::to_string(task).unwrap_or_default(), seq, i64::from(builtin), now()],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn list(&self) -> Vec<Trigger> {
        let rows: Vec<Trigger> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id, name, kind, spec, task, enabled, last_fired, fired, builtin FROM triggers ORDER BY builtin DESC, name")?;
                let v = st
                    .query_map([], |r| {
                        Ok(Trigger {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            kind: r.get(2)?,
                            spec: r.get(3)?,
                            task: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                            enabled: r.get::<_, i64>(5)? == 1,
                            last_fired: r.get(6)?,
                            fired: r.get(7)?,
                            builtin: r.get::<_, i64>(8)? == 1,
                            next: None,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        rows.into_iter()
            .map(|mut t| {
                t.next = match t.kind.as_str() {
                    "cron" => Cron::parse(&t.spec)
                        .ok()
                        .and_then(|c| c.next_after(chrono::Local::now()))
                        .map(|d| d.to_rfc3339()),
                    "interval" => {
                        t.last_fired.as_deref().and_then(|l| chrono::DateTime::parse_from_rfc3339(l).ok()).map(|l| {
                            (l + chrono::Duration::seconds((t.spec.parse::<f64>().unwrap_or(60.0) * 60.0) as i64))
                                .to_rfc3339()
                        })
                    }
                    _ => None,
                };
                t
            })
            .collect()
    }

    pub fn set_enabled(&self, id: &str, on: bool) -> bool {
        self.log
            .db()
            .write(|tx| {
                Ok(tx.execute("UPDATE triggers SET enabled = ?2 WHERE id = ?1", params![id, i64::from(on)])? > 0)
            })
            .unwrap_or(false)
    }

    pub fn remove(&self, id: &str) -> bool {
        self.log
            .db()
            .write(|tx| Ok(tx.execute("DELETE FROM triggers WHERE id = ?1 AND builtin = 0", [id])? > 0))
            .unwrap_or(false)
    }

    fn mark(&self, id: &str, seq: Option<i64>) {
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "UPDATE triggers SET last_fired = ?2, fired = fired + 1, last_seq = COALESCE(?3, last_seq) WHERE id = ?1",
                params![id, now(), seq],
            )?;
            Ok(())
        });
    }

    /// Alles wat nu moet vuren → taken in de queue. Cron haalt een gemist moment één keer in.
    pub fn tick(&self, q: &Queue) -> Vec<Task> {
        let mut out = Vec::new();
        let local = chrono::Local::now();
        let all: Vec<(String, String, String, String, Option<String>, i64)> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id, kind, spec, task, COALESCE(last_fired, created_at), last_seq FROM triggers WHERE enabled = 1")?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let never: Vec<String> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id FROM triggers WHERE last_fired IS NULL")?;
                let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        for (id, kind, spec, task, last, last_seq) in all {
            let mut tpl: NewTask = serde_json::from_str(&task).unwrap_or_default();
            tpl.origin = Some(format!("trigger:{id}"));
            let last_t = last
                .as_deref()
                .and_then(|l| chrono::DateTime::parse_from_rfc3339(l).ok())
                .map(|l| l.with_timezone(&chrono::Local));
            match kind.as_str() {
                "cron" => {
                    let Ok(c) = Cron::parse(&spec) else { continue };
                    if last_t.is_some_and(|l| c.due(l, local)) {
                        // Niet dubbel: dezelfde trigger heeft nog een open taak → overslaan.
                        if !self.has_open(&id) {
                            out.extend(q.enqueue(tpl).ok());
                        }
                        self.mark(&id, None);
                    }
                }
                "interval" => {
                    let mins = spec.parse::<f64>().unwrap_or(60.0);
                    // Nooit gevuurd → meteen (created_at telt alleen voor cron-inhalen).
                    let last_t = if never.contains(&id) { None } else { last_t };
                    if last_t.is_none_or(|l| (local - l).num_seconds() as f64 >= mins * 60.0) {
                        if !self.has_open(&id) {
                            out.extend(q.enqueue(tpl).ok());
                        }
                        self.mark(&id, None);
                    }
                }
                "event" => {
                    let (pattern, filter) = spec.split_once('|').map_or((spec.as_str(), None), |(a, b)| (a, Some(b)));
                    let evs: Vec<(i64, String)> = self
                        .log
                        .db()
                        .read(|c| {
                            let mut st = c.prepare(
                                "SELECT seq, payload FROM events WHERE seq > ?1 AND kind GLOB ?2 ORDER BY seq LIMIT 50",
                            )?;
                            let v = st
                                .query_map(params![last_seq, pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
                                .collect::<rusqlite::Result<Vec<_>>>()?;
                            Ok(v)
                        })
                        .unwrap_or_default();
                    let max_seq: i64 = self
                        .log
                        .db()
                        .read(|c| Ok(c.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?))
                        .unwrap_or(last_seq);
                    let mut fired = false;
                    for (_, payload) in &evs {
                        let p: Value = serde_json::from_str(payload).unwrap_or_default();
                        // Filter "pad=waarde" (bv. status=confirmed).
                        if let Some((k, v)) = filter.and_then(|f| f.split_once('='))
                            && p.pointer(&format!("/{}", k.replace('.', "/")))
                                .map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string()))
                                != Some(v.to_string())
                        {
                            continue;
                        }
                        let mut t = tpl.clone();
                        if let Some(o) = t.params.as_object_mut() {
                            o.insert("event".into(), p.clone());
                        } else {
                            t.params = json!({"event": p});
                        }
                        out.extend(q.enqueue(t).ok());
                        fired = true;
                    }
                    if fired {
                        self.mark(&id, Some(max_seq));
                    } else {
                        let _ = self.log.db().write(|tx| {
                            tx.execute("UPDATE triggers SET last_seq = ?2 WHERE id = ?1", params![id, max_seq])?;
                            Ok(())
                        });
                    }
                }
                _ => {}
            }
        }
        for t in &out {
            let _ = self.log.append(Event::new(
                EventKind::TriggerFired,
                TraceId::new(),
                json!({"task": t.id, "kind": t.kind, "origin": t.origin}),
            ));
        }
        out
    }

    fn has_open(&self, trigger: &str) -> bool {
        let origin = format!("trigger:{trigger}");
        self.log
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE origin = ?1 AND status IN ('queued','running','paused')",
                    [&origin],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap_or(0)
            > 0
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vortex_core::Db;
    use vortex_core::db::CORE_MIGRATIONS;

    use super::*;

    fn q() -> (EventLog, Queue) {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap_or_else(|e| panic!("{e}"))));
        (log.clone(), Queue::new(log))
    }

    fn nt(title: &str, prio: i64) -> NewTask {
        NewTask {
            title: title.into(),
            kind: "prompt".into(),
            priority: prio,
            needs_idle: Some(false),
            ..Default::default()
        }
    }

    #[test]
    fn priority_dependencies_retry_recover() {
        let (_, q) = q();
        let a = q.enqueue(nt("a", 3)).unwrap_or_else(|e| panic!("{e}"));
        let b = q.enqueue(NewTask { depends_on: vec![a.id.clone()], ..nt("b", 1) }).unwrap_or_else(|e| panic!("{e}"));
        let c = q.enqueue(nt("c", 2)).unwrap_or_else(|e| panic!("{e}"));
        // b heeft prioriteit 1 maar wacht op a → eerst c (2), dan a (3).
        assert_eq!(q.next_runnable(true, &[]).map(|t| t.id), Some(c.id.clone()));
        q.start(&c.id, None);
        q.finish(&c.id, Ok(json!({"ok": true})));
        assert_eq!(q.next_runnable(true, &[]).map(|t| t.id), Some(a.id.clone()));
        q.start(&a.id, None);
        q.finish(&a.id, Err("tijdelijk".into()));
        let a2 = q.get(&a.id).unwrap_or_else(|| panic!("a"));
        assert_eq!(a2.status, "queued", "herhaalpoging");
        assert!(a2.not_before.is_some(), "backoff");
        assert!(q.next_runnable(true, &[]).is_none(), "a wacht op backoff, b op a");
        // Herstart midden in een taak → terug in de wachtrij.
        q.start(&b.id, None);
        assert_eq!(q.recover(), 1);
        assert_eq!(q.get(&b.id).map(|t| t.status), Some("queued".into()));
        // Idle-eis.
        let idle = q
            .enqueue(NewTask { title: "zwaar".into(), kind: "prompt".into(), priority: 3, ..Default::default() })
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(idle.needs_idle);
        assert!(q.next_runnable(false, &[]).is_none_or(|t| t.id != idle.id));
        // Afhankelijkheid mislukt → ook mislukt.
        let x = q.enqueue(NewTask { max_attempts: Some(1), ..nt("x", 1) }).unwrap_or_else(|e| panic!("{e}"));
        let y = q.enqueue(NewTask { depends_on: vec![x.id.clone()], ..nt("y", 1) }).unwrap_or_else(|e| panic!("{e}"));
        q.start(&x.id, None);
        q.finish(&x.id, Err("kapot".into()));
        let _ = q.next_runnable(true, &[]);
        assert_eq!(q.get(&y.id).map(|t| t.status), Some("failed".into()));
        assert!(q.enqueue(NewTask { depends_on: vec!["bestaat-niet".into()], ..nt("z", 1) }).is_err());
    }

    #[test]
    fn projects_keep_notes_and_progress() {
        let (_, q) = q();
        let p = q.create_project("Boek schrijven", "hoofdstukken", None).unwrap_or_else(|e| panic!("{e}"));
        let pid = p["id"].as_str().unwrap_or_default().to_string();
        let t = q.enqueue(NewTask { project_id: Some(pid.clone()), ..nt("h1", 2) }).unwrap_or_else(|e| panic!("{e}"));
        q.enqueue(NewTask { project_id: Some(pid.clone()), ..nt("h2", 2) }).unwrap_or_else(|e| panic!("{e}"));
        q.start(&t.id, None);
        q.finish(&t.id, Ok(json!({})));
        q.note(&pid, "h1 af; volgende: h2");
        let p = q.project(&pid).unwrap_or_default();
        assert_eq!(p["progress"], 0.5);
        assert!(p["notes"].as_str().unwrap_or("").contains("volgende: h2"));
    }

    #[test]
    fn triggers_interval_cron_event() {
        let (log, q) = q();
        let tr = Triggers::new(log.clone());
        let tpl = nt("briefing", 2);
        tr.add("elk uur", "interval", "60", &tpl, false).unwrap_or_else(|e| panic!("{e}"));
        let ev = tr
            .add(
                "bij conflict",
                "event",
                "memory.conflict.*",
                &NewTask { title: "conflict oplossen".into(), ..tpl.clone() },
                false,
            )
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(tr.add("fout", "cron", "99 * * * *", &tpl, false).is_err());
        // Interval: eerste tick vuurt (nooit gevuurd), tweede niet; open taak → niet dubbel.
        let first = tr.tick(&q);
        assert_eq!(first.len(), 1);
        assert!(tr.tick(&q).is_empty());
        // Event-trigger: alleen nieuwe events met passend kind.
        let _ = log.append(Event::new(EventKind::ConflictDetected, TraceId::new(), json!({"a": 1})));
        let _ = log.append(Event::new(EventKind::TaskUpdated, TraceId::new(), json!({})));
        let fired = tr.tick(&q);
        assert_eq!(fired.len(), 1, "{fired:?}");
        assert_eq!(fired[0].params["event"]["a"], 1);
        assert!(tr.tick(&q).is_empty(), "niet opnieuw");
        assert!(tr.list().iter().any(|t| t.id == ev && t.fired == 1));
        // Cron die gemist is (aangemaakt "gisteren") → één keer inhalen.
        let id = tr
            .add("dagelijks", "cron", "0 0 * * *", &NewTask { title: "nacht".into(), ..tpl }, false)
            .unwrap_or_else(|e| panic!("{e}"));
        let _ = log.db().write(|tx| {
            tx.execute(
                "UPDATE triggers SET created_at = ?2 WHERE id = ?1",
                params![id, (vortex_types::now() - chrono::Duration::days(2)).to_rfc3339()],
            )?;
            Ok(())
        });
        let c = tr.tick(&q);
        assert_eq!(c.iter().filter(|t| t.title == "nacht").count(), 1);
        assert!(tr.tick(&q).iter().all(|t| t.title != "nacht"));
    }
}
