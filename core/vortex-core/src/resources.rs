//! Resource ownership: elke GPU-reservering, proces, handle of lock heeft een eigenaar en wordt
//! vrijgegeven bij einde of crash. Persistent, zodat het crash-rapport weet wat vastgehouden werd.
// VX:W01.09

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventKind, ResourceId, RunId, TraceId};

use crate::error::Result;
use crate::events::EventLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    GpuSlot,
    Process,
    Handle,
    Lock,
    Stream,
}

impl ResourceKind {
    fn as_str(self) -> &'static str {
        match self {
            ResourceKind::GpuSlot => "gpu_slot",
            ResourceKind::Process => "process",
            ResourceKind::Handle => "handle",
            ResourceKind::Lock => "lock",
            ResourceKind::Stream => "stream",
        }
    }
}

/// Een vastgehouden resource zoals opgeslagen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeldResource {
    pub id: ResourceId,
    pub kind: String,
    pub name: String,
    pub owner_run: Option<RunId>,
    pub owner: String,
    pub detail: serde_json::Value,
    pub acquired_at: String,
}

/// Register van resources.
#[derive(Debug, Clone)]
pub struct ResourceRegistry {
    log: EventLog,
}

impl ResourceRegistry {
    pub fn new(log: EventLog) -> Self {
        Self { log }
    }

    /// Registreer een resource. Vrijgeven gebeurt bij drop van de guard (ook bij panic).
    pub fn acquire(
        &self,
        kind: ResourceKind,
        name: &str,
        owner: &str,
        owner_run: Option<RunId>,
        detail: serde_json::Value,
    ) -> Result<ResourceGuard> {
        let id = ResourceId::new();
        self.log.transact(|tx, out| {
            tx.execute(
                "INSERT INTO resources(id, kind, name, owner_run, owner, detail, acquired_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id.to_string(),
                    kind.as_str(),
                    name,
                    owner_run.map(|r| r.to_string()),
                    owner,
                    serde_json::to_string(&detail)?,
                    vortex_types::now().to_rfc3339()
                ],
            )?;
            let mut e = Event::new(
                EventKind::ResourceAcquired,
                TraceId::new(),
                serde_json::json!({"resource": id, "kind": kind, "name": name, "owner": owner, "detail": detail}),
            );
            e.run_id = owner_run;
            out.emit(tx, e)?;
            Ok(())
        })?;
        Ok(ResourceGuard { id, registry: Some(Arc::new(self.clone())) })
    }

    /// Expliciet vrijgeven (idempotent).
    pub fn release(&self, id: ResourceId) -> Result<bool> {
        self.log.transact(|tx, out| {
            let n = tx.execute("DELETE FROM resources WHERE id = ?1", [id.to_string()])?;
            if n == 1 {
                out.emit(
                    tx,
                    Event::new(EventKind::ResourceReleased, TraceId::new(), serde_json::json!({"resource": id})),
                )?;
            }
            Ok(n == 1)
        })
    }

    /// Alles wat nu vastgehouden wordt.
    pub fn held(&self) -> Result<Vec<HeldResource>> {
        self.log.db().read(|c| {
            let mut st = c.prepare(
                "SELECT id, kind, name, owner_run, owner, detail, acquired_at FROM resources ORDER BY acquired_at",
            )?;
            let rows = st.query_map([], |r| {
                let id: String = r.get(0)?;
                let run: Option<String> = r.get(3)?;
                let detail: String = r.get(5)?;
                Ok((id, r.get(1)?, r.get(2)?, run, r.get(4)?, detail, r.get(6)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, kind, name, run, owner, detail, at): (
                    String,
                    String,
                    String,
                    Option<String>,
                    String,
                    String,
                    String,
                ) = row?;
                out.push(HeldResource {
                    id: id
                        .parse()
                        .map_err(|e: vortex_types::ids::IdParseError| crate::error::CoreError::Config(e.to_string()))?,
                    kind,
                    name,
                    owner_run: run.and_then(|r| r.parse().ok()),
                    owner,
                    detail: serde_json::from_str(&detail)?,
                    acquired_at: at,
                });
            }
            Ok(out)
        })
    }

    /// Alles van één run vrijgeven (einde run). Geeft aantal terug.
    pub fn release_run(&self, run: RunId) -> Result<usize> {
        let ids: Vec<ResourceId> =
            self.held()?.into_iter().filter(|r| r.owner_run == Some(run)).map(|r| r.id).collect();
        for id in &ids {
            self.release(*id)?;
        }
        Ok(ids.len())
    }
}

/// Houdt een resource vast; geeft hem vrij bij drop.
#[derive(Debug)]
pub struct ResourceGuard {
    id: ResourceId,
    registry: Option<Arc<ResourceRegistry>>,
}

impl ResourceGuard {
    pub fn id(&self) -> ResourceId {
        self.id
    }
    /// Laat de registratie staan (bv. voor een proces dat bewust blijft draaien).
    pub fn leak(mut self) -> ResourceId {
        self.registry = None;
        self.id
    }
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        if let Some(reg) = self.registry.take()
            && let Err(e) = reg.release(self.id)
        {
            tracing::error!(resource = %self.id, error = %e, "resource vrijgeven mislukt");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};

    fn reg() -> Result<ResourceRegistry> {
        Ok(ResourceRegistry::new(EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?))))
    }

    #[test]
    fn guard_releases_on_drop() -> Result<()> {
        let r = reg()?;
        {
            let _g = r.acquire(ResourceKind::GpuSlot, "5060ti", "test", None, serde_json::json!({}))?;
            assert_eq!(r.held()?.len(), 1);
        }
        assert!(r.held()?.is_empty());
        Ok(())
    }

    #[test]
    fn guard_releases_on_panic() -> Result<()> {
        let r = reg()?;
        let r2 = r.clone();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _g = r2.acquire(ResourceKind::Lock, "x", "test", None, serde_json::json!({}));
            panic!("boem");
        }));
        assert!(res.is_err());
        assert!(r.held()?.is_empty());
        Ok(())
    }

    #[test]
    fn release_run_frees_only_that_run() -> Result<()> {
        let r = reg()?;
        let a = RunId::new();
        let b = RunId::new();
        r.acquire(ResourceKind::Process, "p1", "t", Some(a), serde_json::json!({"pid": 1}))?.leak();
        r.acquire(ResourceKind::Process, "p2", "t", Some(b), serde_json::json!({"pid": 2}))?.leak();
        assert_eq!(r.release_run(a)?, 1);
        assert_eq!(r.held()?.len(), 1);
        assert!(!r.release(ResourceId::new())?, "onbekend vrijgeven is geen fout");
        Ok(())
    }
}
