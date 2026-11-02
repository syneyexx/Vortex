//! Basis-onderhoud (W05.14): decay (belang × halfwaardetijd + gebruik), opruimen van verlopen
//! kortetermijngeheugen, promotie naar langetermijn, en sessie-samenvattingen (afgeleid, dus
//! vergeten/bewerken van een bron werkt door). Uitgebreide "dreaming" volgt in W12.
// VX:W05.14

use chrono::Utc;
use serde::Serialize;
use vortex_types::{MemoryId, Provenance, SourceKind};

use crate::error::Result;
use crate::store::MemoryStore;
use crate::types::*;

#[derive(Debug, Clone, Default, Serialize)]
pub struct DecayReport {
    pub scanned: usize,
    pub promoted: Vec<MemoryId>,
    pub pruned: Vec<MemoryId>,
}

/// Effectief belang na decay.
pub fn effective_importance(importance: f64, age_days: f64, access_count: i64, halflife_days: f64) -> f64 {
    importance * 0.5f64.powf(age_days / halflife_days.max(0.01)) + 0.1 * ((access_count as f64) + 1.0).ln()
}

impl MemoryStore {
    /// Decay-ronde: kortetermijn ouder dan `short_term_hours` → promoveren (als belangrijk of
    /// gebruikt) of vergeten.
    pub fn decay(&self, short_term_hours: f64, promote_at: f64) -> Result<DecayReport> {
        let mut rep = DecayReport::default();
        let rows = self.list(
            &Filters { kinds: vec![MemoryKind::ShortTerm], include_stale: true, ..Default::default() },
            None,
            10_000,
            0,
        )?;
        let now = Utc::now();
        for m in rows {
            rep.scanned += 1;
            let age_h = chrono::DateTime::parse_from_rfc3339(&m.created_at)
                .map(|t| (now - t.with_timezone(&Utc)).num_minutes() as f64 / 60.0)
                .unwrap_or(0.0);
            if age_h < short_term_hours {
                continue;
            }
            let eff = effective_importance(m.importance, age_h / 24.0, m.access_count, self.config().halflife_days);
            if eff >= promote_at || m.access_count > 0 {
                self.log.db().write(|tx| {
                    tx.execute("UPDATE memories SET kind = 'long_term' WHERE id = ?1", [m.id.to_string()])?;
                    Ok(())
                })?;
                if let Some(x) = self.meta.write().get_mut(&m.id) {
                    x.kind = MemoryKind::LongTerm;
                }
                rep.promoted.push(m.id);
            } else {
                rep.pruned.extend(self.forget(m.id)?);
            }
        }
        Ok(rep)
    }

    /// Samenvatting van een sessie als afgeleide herinnering (episodisch → samenvatting).
    pub async fn summarize_session(&self, session_id: &str, workspace: Option<String>) -> Result<Option<MemoryId>> {
        let items: Vec<Memory> = self
            .list(
                &Filters {
                    kinds: vec![MemoryKind::Episodic, MemoryKind::User],
                    workspace: workspace.clone(),
                    include_stale: true,
                    ..Default::default()
                },
                None,
                500,
                0,
            )?
            .into_iter()
            .filter(|m| m.session_id.as_deref() == Some(session_id))
            .collect();
        if items.is_empty() {
            return Ok(None);
        }
        let texts: Vec<String> = items.iter().map(|m| m.content.clone()).collect();
        let summary = self.llm.summarize(&texts, 120).await?;
        if summary.trim().is_empty() {
            return Ok(None);
        }
        let mut nm = NewMemory::new(
            MemoryKind::Summary,
            summary,
            Provenance::with_origin(SourceKind::Memory, format!("sessie:{session_id}")),
        );
        nm.title = format!("Samenvatting sessie {session_id}");
        nm.workspace = workspace;
        nm.derived_from = items.iter().map(|m| m.id).collect();
        nm.force = true;
        match self.write(nm).await? {
            WriteOutcome::Stored { ids, .. } => Ok(ids.first().copied()),
            WriteOutcome::Merged { into } => Ok(Some(into)),
            WriteOutcome::Skipped { .. } => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_halves_per_halflife_and_use_helps() {
        let a = effective_importance(0.8, 0.0, 0, 30.0);
        let b = effective_importance(0.8, 30.0, 0, 30.0);
        assert!((a / b - 2.0).abs() < 1e-9);
        assert!(effective_importance(0.8, 30.0, 5, 30.0) > b);
    }
}
