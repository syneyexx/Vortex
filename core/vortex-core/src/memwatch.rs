//! RAM-budget per modus (Normaal / Deep) en bewaking per proces (W04.03).
//! Windows: harde cap via Job Object ([`crate::platform::cap_memory`]). Overal: RSS-bewaking —
//! boven de cap → event + callback (de eigenaar maakt een checkpoint en herstart netjes).
// VX:W04.03

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, TraceId};

use crate::events::EventLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RamMode {
    Normal,
    Deep,
}

/// Startwaarden uit VAULT/modules/orkestrator-performance.md (MiB); bijstellen na meting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamBudget {
    pub table: HashMap<RamMode, HashMap<String, u64>>,
}

impl Default for RamBudget {
    fn default() -> Self {
        let gb = |x: f64| (x * 1024.0) as u64;
        let normal = HashMap::from([
            ("os".to_string(), gb(6.0)),
            ("core_gui".to_string(), gb(1.5)),
            ("python_worker".to_string(), gb(3.0)),
            ("db_caches".to_string(), gb(3.0)),
            ("model_ram_cache".to_string(), gb(12.0)),
            ("moe_experts".to_string(), 0),
            ("reserve".to_string(), gb(6.0)),
        ]);
        let deep = HashMap::from([
            ("os".to_string(), gb(6.0)),
            ("core_gui".to_string(), gb(1.5)),
            ("python_worker".to_string(), gb(1.5)),
            ("db_caches".to_string(), gb(2.0)),
            ("model_ram_cache".to_string(), 0),
            ("moe_experts".to_string(), gb(14.0)),
            ("reserve".to_string(), gb(7.0)),
        ]);
        Self { table: HashMap::from([(RamMode::Normal, normal), (RamMode::Deep, deep)]) }
    }
}

impl RamBudget {
    pub fn cap(&self, mode: RamMode, post: &str) -> Option<u64> {
        self.table.get(&mode).and_then(|t| t.get(post)).copied()
    }
    pub fn total(&self, mode: RamMode) -> u64 {
        self.table.get(&mode).map(|t| t.values().sum()).unwrap_or(0)
    }
}

/// RSS van een proces in MiB (None = bestaat niet).
pub fn rss_mib(sys: &mut System, pid: u32) -> Option<u64> {
    let p = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::new().with_memory());
    sys.process(p).map(|pr| pr.memory() / (1024 * 1024))
}

/// Bewaak `pid`: als RSS > `cap_mib` gedurende `grace`, roep `on_exceed` aan (één keer per overschrijding).
pub fn watch_process<F>(
    pid: u32,
    name: String,
    cap_mib: u64,
    grace: Duration,
    log: EventLog,
    stop: CancellationToken,
    on_exceed: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(u64) + Send + 'static,
{
    tokio::spawn(async move {
        let mut sys = System::new();
        let mut over_since: Option<tokio::time::Instant> = None;
        let mut fired = false;
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let Some(rss) = rss_mib(&mut sys, pid) else { break };
            if rss > cap_mib {
                let since = *over_since.get_or_insert_with(tokio::time::Instant::now);
                if !fired && since.elapsed() >= grace {
                    fired = true;
                    let _ = log.append(Event::new(
                        EventKind::MemoryCapExceeded,
                        TraceId::new(),
                        serde_json::json!({"pid": pid, "process": name, "rss_mib": rss, "cap_mib": cap_mib}),
                    ));
                    tracing::warn!(pid, rss, cap_mib, "geheugencap overschreden");
                    on_exceed(rss);
                }
            } else {
                over_since = None;
                fired = false;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn budget_fits_32gb_in_both_modes() {
        let b = RamBudget::default();
        assert!(b.total(RamMode::Normal) <= 32 * 1024);
        assert!(b.total(RamMode::Deep) <= 32 * 1024);
        assert_eq!(b.cap(RamMode::Deep, "model_ram_cache"), Some(0));
    }

    #[tokio::test]
    async fn watcher_fires_when_over_cap() -> crate::Result<()> {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?));
        let hit = Arc::new(AtomicU64::new(0));
        let h2 = hit.clone();
        let stop = CancellationToken::new();
        // Eigen proces met cap 1 MiB: zeker overschreden.
        let jh = watch_process(
            std::process::id(),
            "test".into(),
            1,
            Duration::from_millis(100),
            log.clone(),
            stop.clone(),
            move |rss| {
                h2.store(rss, Ordering::SeqCst);
            },
        );
        for _ in 0..40 {
            if hit.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        stop.cancel();
        let _ = jh.await;
        assert!(hit.load(Ordering::SeqCst) > 1);
        assert!(log.read_from(0, 10)?.iter().any(|e| e.kind == EventKind::MemoryCapExceeded));
        Ok(())
    }
}
