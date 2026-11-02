//! Leeragenda (W07.08): kennisgaten, betwiste feiten en zwakke taaktypen registreren; in idle
//! (W15) onderzoekt Vortex ze zelf.
// VX:W07.08

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventId, EventKind, TraceId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaItem {
    pub id: String,
    pub topic: String,
    pub reason: String,
    pub detail: String,
    pub priority: f64,
    pub status: String,
    pub hits: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Agenda {
    log: EventLog,
}

impl Agenda {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    /// Registreer (of versterk) een agendapunt. Herhaling verhoogt de prioriteit.
    pub fn add(&self, topic: &str, reason: &str, detail: &str, priority: f64) -> Result<(), String> {
        let topic: String = topic.trim().chars().take(200).collect();
        if topic.is_empty() {
            return Ok(());
        }
        let now = vortex_types::now().to_rfc3339();
        self.log
            .transact(|tx, out| {
                tx.execute(
                    "INSERT INTO learning_agenda(id, topic, reason, detail, priority, status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?6)
                     ON CONFLICT(topic, reason) DO UPDATE SET hits = hits + 1, priority = MIN(1.0, priority + 0.1), updated_at = ?6,
                        status = CASE WHEN status = 'done' THEN 'open' ELSE status END",
                    params![EventId::new().to_string(), topic, reason, detail, priority, now],
                )?;
                out.emit(tx, Event::new(EventKind::AgendaItemAdded, TraceId::new(), serde_json::json!({"topic": topic, "reason": reason})))?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn list(&self, status: Option<&str>, limit: usize) -> Result<Vec<AgendaItem>, String> {
        self.log
            .db()
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT id, topic, reason, detail, priority, status, hits, updated_at FROM learning_agenda
                     WHERE (?1 IS NULL OR status = ?1) ORDER BY priority DESC, hits DESC LIMIT ?2",
                )?;
                let rows = st.query_map(params![status, limit as i64], |r| {
                    Ok(AgendaItem {
                        id: r.get(0)?,
                        topic: r.get(1)?,
                        reason: r.get(2)?,
                        detail: r.get(3)?,
                        priority: r.get(4)?,
                        status: r.get(5)?,
                        hits: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                })?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .map_err(|e| e.to_string())
    }

    pub fn set_status(&self, id: &str, status: &str) -> Result<(), String> {
        self.log
            .db()
            .write(|tx| {
                Ok(tx.execute(
                    "UPDATE learning_agenda SET status = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, status, vortex_types::now().to_rfc3339()],
                )?)
            })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vortex_core::{Db, db::CORE_MIGRATIONS};

    #[test]
    fn repeated_gaps_raise_priority() -> Result<(), String> {
        let a = Agenda::new(EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).map_err(|e| e.to_string())?)));
        a.add("CUDA-versies voor Pascal", "gap", "geen antwoord gevonden", 0.5)?;
        a.add("CUDA-versies voor Pascal", "gap", "", 0.5)?;
        a.add("NVFP4 kwaliteit", "disputed", "", 0.4)?;
        let l = a.list(Some("open"), 10)?;
        assert_eq!(l[0].topic, "CUDA-versies voor Pascal");
        assert_eq!(l[0].hits, 2);
        assert!(l[0].priority > 0.55);
        a.set_status(&l[0].id, "done")?;
        assert_eq!(a.list(Some("open"), 10)?.len(), 1);
        Ok(())
    }
}
