//! Runs: state machine met overgangsvoorwaarden, optimistic concurrency, snapshot-reads.
//! Elke overgang schrijft status + event in één transactie (outbox).
// VX:W01.04

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventKind, Priority, RunId, RunKind, RunState, StopStatus, Timestamp, TraceId};

use crate::budget::{Budget, Usage};
use crate::error::{CoreError, Result};
use crate::events::{EventLog, Outbox};

/// Toegestane overgangen (normale werking). Herstel na crash gebruikt [`RECOVERY_TRANSITIONS`].
pub fn allowed(from: RunState, to: RunState) -> bool {
    use RunState::*;
    matches!(
        (from, to),
        (Queued, Starting | Cancelled | Failed | Cancelling)
            | (Starting, Running | Failed | Cancelling)
            | (Running, Waiting | Completed | Failed | Cancelling)
            | (Waiting, Running | Failed | Cancelling)
            | (Cancelling, Cancelled | Failed)
    )
}

/// Alleen voor crash-herstel: een niet-afgeronde run terug in de wachtrij of afsluiten.
pub const RECOVERY_TRANSITIONS: &[(RunState, RunState)] = &[
    (RunState::Starting, RunState::Queued),
    (RunState::Running, RunState::Queued),
    (RunState::Waiting, RunState::Queued),
    (RunState::Starting, RunState::Failed),
    (RunState::Running, RunState::Failed),
    (RunState::Waiting, RunState::Failed),
    (RunState::Cancelling, RunState::Cancelled),
];

/// Wat nodig is om een run aan te maken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSpec {
    pub kind: RunKind,
    pub priority: Priority,
    pub parent: Option<RunId>,
    pub trace_id: Option<TraceId>,
    pub budget: Budget,
    pub deadline: Option<Timestamp>,
    pub resumable: bool,
    pub input: serde_json::Value,
}

impl RunSpec {
    pub fn new(kind: RunKind, priority: Priority) -> Self {
        Self {
            kind,
            priority,
            parent: None,
            trace_id: None,
            budget: Budget::default(),
            deadline: None,
            resumable: false,
            input: serde_json::Value::Null,
        }
    }
}

/// Opgeslagen toestand van een run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub kind: RunKind,
    pub priority: Priority,
    pub state: RunState,
    pub version: i64,
    pub parent: Option<RunId>,
    pub trace_id: TraceId,
    pub config_version: Option<String>,
    pub resumable: bool,
    pub stop_status: Option<StopStatus>,
    pub deadline: Option<Timestamp>,
    pub budget: Budget,
    pub usage: Usage,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

fn prio_from(i: i64) -> Priority {
    match i {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    }
}

fn stop_str(s: StopStatus) -> &'static str {
    match s {
        StopStatus::Requested => "requested",
        StopStatus::Stopped => "stopped",
        StopStatus::CleanedUp => "cleaned_up",
    }
}

fn stop_parse(s: &str) -> Option<StopStatus> {
    match s {
        "requested" => Some(StopStatus::Requested),
        "stopped" => Some(StopStatus::Stopped),
        "cleaned_up" => Some(StopStatus::CleanedUp),
        _ => None,
    }
}

const SELECT: &str = "SELECT id, kind, priority, state, version, parent_id, trace_id, config_version, resumable,
    stop_status, deadline, budget, usage, input, output, error, created_at, updated_at FROM runs";

fn conv<T, E: std::fmt::Display>(i: usize, v: std::result::Result<T, E>) -> rusqlite::Result<T> {
    v.map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.to_string().into()))
}

fn ts(i: usize, s: String) -> rusqlite::Result<Timestamp> {
    Ok(conv(i, chrono::DateTime::parse_from_rfc3339(&s))?.with_timezone(&chrono::Utc))
}

fn row_to_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let state: String = r.get(3)?;
    let parent: Option<String> = r.get(5)?;
    let stop: Option<String> = r.get(9)?;
    let deadline: Option<String> = r.get(10)?;
    let output: Option<String> = r.get(14)?;
    Ok(RunRecord {
        id: conv(0, r.get::<_, String>(0)?.parse())?,
        kind: RunKind::parse(&r.get::<_, String>(1)?),
        priority: prio_from(r.get(2)?),
        state: conv(3, RunState::parse(&state).ok_or(format!("onbekende status {state}")))?,
        version: r.get(4)?,
        parent: parent.map(|p| conv(5, p.parse())).transpose()?,
        trace_id: conv(6, r.get::<_, String>(6)?.parse())?,
        config_version: r.get(7)?,
        resumable: r.get::<_, i64>(8)? != 0,
        stop_status: stop.as_deref().and_then(stop_parse),
        deadline: deadline.map(|d| ts(10, d)).transpose()?,
        budget: conv(11, serde_json::from_str(&r.get::<_, String>(11)?))?,
        usage: conv(12, serde_json::from_str(&r.get::<_, String>(12)?))?,
        input: conv(13, serde_json::from_str(&r.get::<_, String>(13)?))?,
        output: output.map(|o| conv(14, serde_json::from_str(&o))).transpose()?,
        error: r.get(15)?,
        created_at: ts(16, r.get(16)?)?,
        updated_at: ts(17, r.get(17)?)?,
    })
}

/// Opslag en overgangen van runs.
#[derive(Debug, Clone)]
pub struct RunStore {
    log: EventLog,
}

/// Extra data bij een overgang.
#[derive(Debug, Clone, Default)]
pub struct TransitionData {
    pub reason: Option<String>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl RunStore {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    pub fn log(&self) -> &EventLog {
        &self.log
    }

    /// Nieuwe run (status `queued`).
    pub fn create(&self, spec: &RunSpec, config_version: Option<&str>) -> Result<RunRecord> {
        let id = RunId::new();
        let trace = spec.trace_id.unwrap_or_default();
        let now = vortex_types::now();
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO runs(id, kind, priority, state, version, parent_id, trace_id, config_version, resumable,
                    deadline, budget, usage, input, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'queued', 1, ?4, ?5, ?6, ?7, ?8, ?9, '{}', ?10, ?11, ?11)",
                params![
                    id.to_string(),
                    spec.kind.as_str(),
                    spec.priority as i64,
                    spec.parent.map(|p| p.to_string()),
                    trace.to_string(),
                    config_version,
                    spec.resumable as i64,
                    spec.deadline.map(|d| d.to_rfc3339()),
                    serde_json::to_string(&spec.budget)?,
                    serde_json::to_string(&spec.input)?,
                    now.to_rfc3339(),
                ],
            )?;
            out.emit(
                tx,
                Event::new(
                    EventKind::RunCreated,
                    trace,
                    serde_json::json!({
                        "kind": spec.kind.as_str(),
                        "priority": spec.priority,
                        "parent": spec.parent,
                        "config_version": config_version,
                    }),
                )
                .for_run(id),
            )?;
            get_tx(tx, id)
        })
    }

    /// Snapshot-read van één run.
    pub fn get(&self, id: RunId) -> Result<RunRecord> {
        self.log.db().read(|c| {
            c.query_row(&format!("{SELECT} WHERE id = ?1"), [id.to_string()], row_to_run)
                .optional()?
                .ok_or_else(|| CoreError::NotFound(id.to_string()))
        })
    }

    /// Alle runs die nog niet af zijn (snapshot-consistent).
    pub fn active(&self) -> Result<Vec<RunRecord>> {
        self.log.db().read(|c| {
            let mut st = c.prepare(&format!(
                "{SELECT} WHERE state NOT IN ('completed','failed','cancelled') ORDER BY created_at"
            ))?;
            let rows = st.query_map([], row_to_run)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Laatste `limit` runs.
    pub fn recent(&self, limit: usize) -> Result<Vec<RunRecord>> {
        self.log.db().read(|c| {
            let mut st = c.prepare(&format!("{SELECT} ORDER BY created_at DESC, id DESC LIMIT ?1"))?;
            let rows = st.query_map([limit as i64], row_to_run)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Kinderen van een run.
    pub fn children(&self, parent: RunId) -> Result<Vec<RunRecord>> {
        self.log.db().read(|c| {
            let mut st = c.prepare(&format!("{SELECT} WHERE parent_id = ?1"))?;
            let rows = st.query_map([parent.to_string()], row_to_run)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Overgang met voorwaarden. `expected_version` = optimistic concurrency (None = huidige versie).
    pub fn transition(
        &self,
        id: RunId,
        to: RunState,
        expected_version: Option<i64>,
        data: TransitionData,
    ) -> Result<RunRecord> {
        self.log.transact(|tx, out| transition_tx(tx, out, id, to, expected_version, data, false))
    }

    /// Overgang tijdens crash-herstel (alleen [`RECOVERY_TRANSITIONS`]).
    pub fn recover_transition(&self, id: RunId, to: RunState, reason: &str) -> Result<RunRecord> {
        self.log.transact(|tx, out| {
            transition_tx(
                tx,
                out,
                id,
                to,
                None,
                TransitionData {
                    reason: Some(reason.into()),
                    error: (to == RunState::Failed).then(|| reason.into()),
                    output: None,
                },
                true,
            )
        })
    }

    /// Stopstatus vastleggen (aangevraagd → gestopt → opgeruimd), met event.
    pub fn set_stop_status(&self, id: RunId, status: StopStatus) -> Result<()> {
        self.log.transact(|tx, out| {
            let run = get_tx(tx, id)?;
            if run.stop_status.is_some_and(|s| s >= status) {
                return Ok(()); // idempotent, nooit terug
            }
            tx.execute(
                "UPDATE runs SET stop_status = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), stop_str(status), vortex_types::now().to_rfc3339()],
            )?;
            let kind = match status {
                StopStatus::Requested => EventKind::RunStopRequested,
                StopStatus::Stopped => EventKind::RunStopped,
                StopStatus::CleanedUp => EventKind::RunCleanedUp,
            };
            out.emit(tx, Event::new(kind, run.trace_id, serde_json::json!({})).for_run(id))?;
            Ok(())
        })
    }

    /// Verbruik bijwerken (tokens/stappen/tijd) — los van statusovergangen.
    pub fn record_usage(&self, id: RunId, usage: &Usage) -> Result<()> {
        self.log.db().write(|tx| {
            tx.execute(
                "UPDATE runs SET usage = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), serde_json::to_string(usage)?, vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }
}

pub(crate) fn get_tx(tx: &Transaction<'_>, id: RunId) -> Result<RunRecord> {
    tx.query_row(&format!("{SELECT} WHERE id = ?1"), [id.to_string()], row_to_run)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(id.to_string()))
}

fn transition_tx(
    tx: &Transaction<'_>,
    out: &mut Outbox,
    id: RunId,
    to: RunState,
    expected_version: Option<i64>,
    data: TransitionData,
    recovery: bool,
) -> Result<RunRecord> {
    let run = get_tx(tx, id)?;
    if let Some(v) = expected_version
        && v != run.version
    {
        return Err(CoreError::VersionConflict { expected: v, found: run.version });
    }
    let from = run.state;
    let ok = if recovery { RECOVERY_TRANSITIONS.contains(&(from, to)) } else { allowed(from, to) };
    if !ok {
        return Err(CoreError::InvalidTransition { from, to, reason: "niet toegestaan".into() });
    }
    // Voorwaarden per doelstatus.
    match to {
        RunState::Completed if data.output.is_none() => {
            return Err(CoreError::InvalidTransition {
                from,
                to,
                reason: "completed pas na opslag van definitieve uitvoer".into(),
            });
        }
        RunState::Failed if data.error.is_none() => {
            return Err(CoreError::InvalidTransition { from, to, reason: "failed vereist een foutmelding".into() });
        }
        RunState::Cancelled
            if !recovery
                && from == RunState::Cancelling
                && run.stop_status != Some(StopStatus::Stopped)
                && run.stop_status != Some(StopStatus::CleanedUp) =>
        {
            return Err(CoreError::InvalidTransition {
                from,
                to,
                reason: "cancelled pas als het werk echt gestopt is".into(),
            });
        }
        _ => {}
    }
    let output = data.output.as_ref().map(serde_json::to_string).transpose()?;
    let n = tx.execute(
        "UPDATE runs SET state = ?2, version = version + 1, updated_at = ?3,
            output = COALESCE(?4, output), error = COALESCE(?5, error)
         WHERE id = ?1 AND version = ?6",
        params![id.to_string(), to.as_str(), vortex_types::now().to_rfc3339(), output, data.error, run.version],
    )?;
    if n != 1 {
        return Err(CoreError::VersionConflict { expected: run.version, found: -1 });
    }
    out.emit(
        tx,
        Event::new(
            if recovery && to == RunState::Queued { EventKind::RunResumed } else { EventKind::RunStateChanged },
            run.trace_id,
            serde_json::json!({
                "from": from,
                "to": to,
                "version": run.version + 1,
                "reason": data.reason,
                "error": data.error,
                "recovery": recovery,
            }),
        )
        .for_run(id),
    )?;
    get_tx(tx, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};
    use RunState::*;
    use std::sync::Arc;

    fn store() -> Result<RunStore> {
        Ok(RunStore::new(EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?))))
    }

    fn new_run(s: &RunStore) -> Result<RunRecord> {
        s.create(&RunSpec::new(RunKind::Chat, Priority::P0), Some("cfg-1"))
    }

    fn go(s: &RunStore, id: RunId, to: RunState) -> Result<RunRecord> {
        let data = TransitionData {
            output: (to == Completed).then(|| serde_json::json!("ok")),
            error: (to == Failed).then(|| "x".to_string()),
            reason: None,
        };
        s.transition(id, to, None, data)
    }

    #[test]
    fn transition_table_is_exhaustively_correct() {
        // Elke combinatie expliciet: terminal → niets; geen zelf-overgangen.
        for from in RunState::ALL {
            for to in RunState::ALL {
                if from.is_terminal() || from == to {
                    assert!(!allowed(from, to), "{from:?} → {to:?} mag niet");
                }
            }
        }
        assert!(allowed(Queued, Starting));
        assert!(!allowed(Queued, Running), "queued moet eerst starten");
        assert!(!allowed(Queued, Completed));
        assert!(allowed(Running, Completed));
        assert!(allowed(Waiting, Running));
        assert!(!allowed(Cancelling, Running), "na stopverzoek nooit terug naar running");
    }

    #[test]
    fn happy_path_bumps_version_and_logs_events() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        assert_eq!(r.state, Queued);
        assert_eq!(r.config_version.as_deref(), Some("cfg-1"));
        go(&s, r.id, Starting)?;
        go(&s, r.id, Running)?;
        go(&s, r.id, Waiting)?;
        go(&s, r.id, Running)?;
        let done = go(&s, r.id, Completed)?;
        assert_eq!(done.version, 6);
        assert_eq!(done.output, Some(serde_json::json!("ok")));
        let ev = s.log().for_run(r.id)?;
        assert_eq!(ev.len(), 6);
        assert_eq!(ev[0].kind, EventKind::RunCreated);
        assert!(ev[1..].iter().all(|e| e.kind == EventKind::RunStateChanged));
        Ok(())
    }

    #[test]
    fn completed_requires_output() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        go(&s, r.id, Starting)?;
        go(&s, r.id, Running)?;
        let e = s.transition(r.id, Completed, None, TransitionData::default());
        assert!(matches!(e, Err(CoreError::InvalidTransition { .. })));
        assert_eq!(s.get(r.id)?.state, Running, "status onveranderd na geweigerde overgang");
        Ok(())
    }

    #[test]
    fn failed_requires_error() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        assert!(s.transition(r.id, Failed, None, TransitionData::default()).is_err());
        Ok(())
    }

    #[test]
    fn cancelled_requires_stopped() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        go(&s, r.id, Starting)?;
        go(&s, r.id, Running)?;
        s.set_stop_status(r.id, StopStatus::Requested)?;
        go(&s, r.id, Cancelling)?;
        assert!(go(&s, r.id, Cancelled).is_err(), "nog niet echt gestopt");
        s.set_stop_status(r.id, StopStatus::Stopped)?;
        assert_eq!(go(&s, r.id, Cancelled)?.state, Cancelled);
        Ok(())
    }

    #[test]
    fn queued_can_be_cancelled_directly() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        assert_eq!(go(&s, r.id, Cancelled)?.state, Cancelled);
        Ok(())
    }

    #[test]
    fn terminal_states_are_final() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        go(&s, r.id, Failed)?;
        for to in RunState::ALL {
            assert!(go(&s, r.id, to).is_err());
        }
        Ok(())
    }

    #[test]
    fn optimistic_concurrency_rejects_stale_version() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        s.transition(r.id, Starting, Some(r.version), TransitionData::default())?;
        let stale = s.transition(r.id, Running, Some(r.version), TransitionData::default());
        assert!(matches!(stale, Err(CoreError::VersionConflict { .. })));
        Ok(())
    }

    #[test]
    fn stop_status_only_moves_forward() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        s.set_stop_status(r.id, StopStatus::Stopped)?;
        s.set_stop_status(r.id, StopStatus::Requested)?;
        assert_eq!(s.get(r.id)?.stop_status, Some(StopStatus::Stopped));
        Ok(())
    }

    #[test]
    fn recovery_transitions_only_from_list() -> Result<()> {
        let s = store()?;
        let r = new_run(&s)?;
        assert!(s.recover_transition(r.id, Queued, "crash").is_err(), "queued→queued is geen herstel");
        go(&s, r.id, Starting)?;
        go(&s, r.id, Running)?;
        let back = s.recover_transition(r.id, Queued, "crash")?;
        assert_eq!(back.state, Queued);
        let ev = s.log().for_run(r.id)?;
        assert_eq!(ev.last().map(|e| e.kind), Some(EventKind::RunResumed));
        Ok(())
    }

    #[test]
    fn children_are_linked_to_parent() -> Result<()> {
        let s = store()?;
        let p = new_run(&s)?;
        let mut spec = RunSpec::new(RunKind::Research, Priority::P1);
        spec.parent = Some(p.id);
        let c = s.create(&spec, None)?;
        assert_eq!(s.children(p.id)?.first().map(|r| r.id), Some(c.id));
        Ok(())
    }
}
