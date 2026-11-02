//! Idempotentie-ledger (W08.04): vóór een actie met bijwerking een 'intent', erna 'done' met de
//! uitvoer. Retry met dezelfde sleutel → opgeslagen resultaat, geen tweede effect.

use rusqlite::{OptionalExtension, params};
use vortex_core::EventLog;

use crate::contract::Effect;
use crate::tool::ToolOutput;

#[derive(Debug, Clone, PartialEq)]
pub enum LedgerState {
    /// Nieuw: uitvoeren.
    Fresh,
    /// Al gedaan: dit is de uitvoer.
    Done(ToolOutput),
    /// Eerder gestart maar nooit afgemeld (crash): mogelijk uitgevoerd.
    Interrupted,
    /// Eerder mislukt: opnieuw proberen mag.
    Failed,
    /// Zelfde sleutel, andere invoer: weigeren.
    Conflict,
}

#[derive(Debug, Clone)]
pub struct Ledger {
    log: EventLog,
}

fn effect_str(e: Effect) -> &'static str {
    match e {
        Effect::Read => "read",
        Effect::Local => "local",
        Effect::External => "external",
    }
}

impl Ledger {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    /// Registreer intentie; geeft de toestand vóór deze aanroep.
    pub fn begin(
        &self,
        key: &str,
        tool: &str,
        input_hash: &str,
        effect: Effect,
        run: Option<String>,
    ) -> Result<LedgerState, String> {
        let now = vortex_types::now().to_rfc3339();
        self.log
            .db()
            .write(|tx| {
                let row: Option<(String, String, Option<String>)> = tx
                    .query_row("SELECT input_hash, state, output FROM tool_ledger WHERE key = ?1", [key], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()?;
                Ok(match row {
                    None => {
                        tx.execute(
                            "INSERT INTO tool_ledger(key, tool, input_hash, effect, state, run_id, created_at, updated_at)
                             VALUES (?1, ?2, ?3, ?4, 'intent', ?5, ?6, ?6)",
                            params![key, tool, input_hash, effect_str(effect), run, now],
                        )?;
                        LedgerState::Fresh
                    }
                    Some((h, _, _)) if h != input_hash => LedgerState::Conflict,
                    Some((_, state, out)) => match state.as_str() {
                        "done" => out
                            .and_then(|o| serde_json::from_str::<ToolOutput>(&o).ok())
                            .map(LedgerState::Done)
                            .unwrap_or(LedgerState::Interrupted),
                        "failed" => {
                            tx.execute(
                                "UPDATE tool_ledger SET state = 'intent', attempts = attempts + 1, updated_at = ?2 WHERE key = ?1",
                                params![key, now],
                            )?;
                            LedgerState::Failed
                        }
                        _ => LedgerState::Interrupted,
                    },
                })
            })
            .map_err(|e| e.to_string())
    }

    /// Opnieuw proberen na een onderbroken intentie (expliciet: alleen als de tool veilig is).
    pub fn retake(&self, key: &str) -> Result<(), String> {
        let now = vortex_types::now().to_rfc3339();
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "UPDATE tool_ledger SET attempts = attempts + 1, updated_at = ?2 WHERE key = ?1",
                    params![key, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn finish(&self, key: &str, result: Result<&ToolOutput, &str>) -> Result<(), String> {
        let now = vortex_types::now().to_rfc3339();
        self.log
            .db()
            .write(|tx| {
                match result {
                    Ok(o) => tx.execute(
                        "UPDATE tool_ledger SET state = 'done', output = ?2, error = NULL, updated_at = ?3 WHERE key = ?1",
                        params![key, serde_json::to_string(o).unwrap_or_default(), now],
                    )?,
                    Err(e) => tx.execute(
                        "UPDATE tool_ledger SET state = 'failed', error = ?2, updated_at = ?3 WHERE key = ?1",
                        params![key, e, now],
                    )?,
                };
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// Aantal pogingen (voor tests/inspectie).
    pub fn attempts(&self, key: &str) -> Option<i64> {
        self.log
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT attempts FROM tool_ledger WHERE key = ?1", [key], |r| r.get(0)).optional()?)
            })
            .ok()
            .flatten()
    }
}
