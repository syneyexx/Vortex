//! Checkpoints (hervatpunten) en effect-registratie (wat mogelijk extern al gebeurd is).
// VX:W01.11

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use vortex_types::{CheckpointId, Event, EventKind, RunId};

use crate::error::Result;
use crate::events::EventLog;
use crate::runs::get_tx;

/// Opgeslagen checkpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub run_id: RunId,
    pub seq: i64,
    pub ts: String,
    pub data: serde_json::Value,
}

/// Soort bijwerking van een stap (uit toolcontracten).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Read,
    Local,
    External,
}

impl EffectKind {
    fn as_str(self) -> &'static str {
        match self {
            EffectKind::Read => "read",
            EffectKind::Local => "local",
            EffectKind::External => "external",
        }
    }
}

/// Een geregistreerd effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectRecord {
    pub id: String,
    pub run_id: RunId,
    pub kind: String,
    pub description: String,
    pub idempotency_key: Option<String>,
    pub state: String,
}

/// Checkpoints en effecten.
#[derive(Debug, Clone)]
pub struct Checkpoints {
    log: EventLog,
}

impl Checkpoints {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    /// Sla een checkpoint op (+ event in dezelfde transactie). Geeft het volgnummer.
    pub fn save(&self, run: RunId, data: &serde_json::Value) -> Result<i64> {
        self.log.transact(|tx, out| {
            let rec = get_tx(tx, run)?;
            let seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM checkpoints WHERE run_id = ?1",
                [run.to_string()],
                |r| r.get(0),
            )?;
            let id = CheckpointId::new();
            tx.execute(
                "INSERT INTO checkpoints(id, run_id, seq, ts, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id.to_string(),
                    run.to_string(),
                    seq,
                    vortex_types::now().to_rfc3339(),
                    serde_json::to_string(data)?
                ],
            )?;
            out.emit(
                tx,
                Event::new(EventKind::RunCheckpointed, rec.trace_id, serde_json::json!({"checkpoint": id, "seq": seq}))
                    .for_run(run),
            )?;
            Ok(seq)
        })
    }

    /// Laatste checkpoint van een run.
    pub fn latest(&self, run: RunId) -> Result<Option<Checkpoint>> {
        self.log.db().read(|c| {
            let row = c
                .query_row(
                    "SELECT id, seq, ts, data FROM checkpoints WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
                    [run.to_string()],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            row.map(|(id, seq, ts, data)| {
                Ok(Checkpoint {
                    id: id
                        .parse()
                        .map_err(|e: vortex_types::ids::IdParseError| crate::error::CoreError::Config(e.to_string()))?,
                    run_id: run,
                    seq,
                    ts,
                    data: serde_json::from_str(&data)?,
                })
            })
            .transpose()
        })
    }

    /// Leg de intentie van een effect vast vóór uitvoeren. Bestaande sleutel = al eerder gepland
    /// (idempotent: geeft het bestaande effect-ID terug).
    pub fn effect_intent(
        &self,
        run: RunId,
        kind: EffectKind,
        description: &str,
        idempotency_key: Option<&str>,
    ) -> Result<String> {
        self.log.db().write(|tx| {
            if let Some(key) = idempotency_key
                && let Some(id) = tx
                    .query_row(
                        "SELECT id FROM effects WHERE run_id = ?1 AND idempotency_key = ?2",
                        params![run.to_string(), key],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
            {
                return Ok(id);
            }
            let id = vortex_types::EventId::new().to_string().replacen("evt_", "eff_", 1);
            let now = vortex_types::now().to_rfc3339();
            tx.execute(
                "INSERT INTO effects(id, run_id, kind, description, idempotency_key, state, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'intent', ?6, ?6)",
                params![id, run.to_string(), kind.as_str(), description, idempotency_key, now],
            )?;
            Ok(id)
        })
    }

    /// Markeer effect als uitgevoerd (`done = true`) of mislukt.
    pub fn effect_finished(&self, effect_id: &str, done: bool) -> Result<()> {
        self.log.db().write(|tx| {
            tx.execute(
                "UPDATE effects SET state = ?2, updated_at = ?3 WHERE id = ?1",
                params![effect_id, if done { "done" } else { "failed" }, vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    /// Effecten van een run met een bepaalde status.
    pub fn effects(&self, run: Option<RunId>, state: Option<&str>) -> Result<Vec<EffectRecord>> {
        self.log.db().read(|c| {
            let mut st = c.prepare(
                "SELECT id, run_id, kind, description, idempotency_key, state FROM effects
                 WHERE (?1 IS NULL OR run_id = ?1) AND (?2 IS NULL OR state = ?2) ORDER BY created_at",
            )?;
            let rows = st.query_map(params![run.map(|r| r.to_string()), state], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, run_id, kind, description, idempotency_key, state) = row?;
                out.push(EffectRecord {
                    id,
                    run_id: run_id
                        .parse()
                        .map_err(|e: vortex_types::ids::IdParseError| crate::error::CoreError::Config(e.to_string()))?,
                    kind,
                    description,
                    idempotency_key,
                    state,
                });
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};
    use crate::runs::{RunSpec, RunStore};
    use std::sync::Arc;
    use vortex_types::{Priority, RunKind};

    fn setup() -> Result<(Checkpoints, RunId)> {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?));
        let run = RunStore::new(log.clone()).create(&RunSpec::new(RunKind::Ingest, Priority::P2), None)?;
        Ok((Checkpoints::new(log), run.id))
    }

    #[test]
    fn checkpoints_are_sequential_and_latest_wins() -> Result<()> {
        let (c, run) = setup()?;
        assert!(c.latest(run)?.is_none());
        assert_eq!(c.save(run, &serde_json::json!({"page": 1}))?, 1);
        assert_eq!(c.save(run, &serde_json::json!({"page": 2}))?, 2);
        assert_eq!(c.latest(run)?.map(|k| k.data["page"].clone()), Some(serde_json::json!(2)));
        Ok(())
    }

    #[test]
    fn effects_with_same_key_are_idempotent() -> Result<()> {
        let (c, run) = setup()?;
        let a = c.effect_intent(run, EffectKind::External, "POST /x", Some("k1"))?;
        let b = c.effect_intent(run, EffectKind::External, "POST /x", Some("k1"))?;
        assert_eq!(a, b);
        assert_eq!(c.effects(Some(run), Some("intent"))?.len(), 1);
        c.effect_finished(&a, true)?;
        assert!(c.effects(Some(run), Some("intent"))?.is_empty());
        Ok(())
    }
}
