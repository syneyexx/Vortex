//! Eenvoudige GPU-slot-scheduler (W01): één taak tegelijk op de hoofd-GPU, chat (P0) eerst,
//! daarna op volgorde van aanmelding. De volledige Dirigent volgt in W04.
// VX:W01.18

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, Priority, RunId, TraceId};

use crate::error::{CoreError, Result};
use crate::events::EventLog;
use crate::resources::{ResourceGuard, ResourceKind, ResourceRegistry};

type Key = (Priority, u64);

#[derive(Debug)]
struct State {
    free: usize,
    next: u64,
    waiting: BTreeMap<Key, (RunId, oneshot::Sender<()>)>,
}

/// Slot-scheduler voor één GPU (of ander exclusief middel).
#[derive(Debug, Clone)]
pub struct SlotScheduler {
    name: Arc<str>,
    state: Arc<Mutex<State>>,
    log: EventLog,
    resources: ResourceRegistry,
}

/// Toegekend slot; geeft het vrij bij drop (en wekt de volgende).
#[derive(Debug)]
pub struct SlotPermit {
    sched: SlotScheduler,
    run: RunId,
    _resource: ResourceGuard,
}

impl Drop for SlotPermit {
    fn drop(&mut self) {
        self.sched.release(self.run);
    }
}

impl SlotScheduler {
    pub fn new(name: &str, slots: usize, log: EventLog, resources: ResourceRegistry) -> Self {
        Self {
            name: name.into(),
            state: Arc::new(Mutex::new(State { free: slots.max(1), next: 0, waiting: BTreeMap::new() })),
            log,
            resources,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Aantal wachtenden.
    pub fn queue_len(&self) -> usize {
        self.state.lock().waiting.len()
    }

    /// Wacht op een slot. Annuleerbaar; P0 gaat voor op alles wat nog wacht.
    pub async fn acquire(&self, run: RunId, priority: Priority, cancel: &CancellationToken) -> Result<SlotPermit> {
        let rx = {
            let mut st = self.state.lock();
            if st.free > 0 && st.waiting.is_empty() {
                st.free -= 1;
                None
            } else {
                let (tx, rx) = oneshot::channel();
                let key = (priority, st.next);
                st.next += 1;
                st.waiting.insert(key, (run, tx));
                Some((key, rx))
            }
        };
        if let Some((key, rx)) = rx {
            tokio::select! {
                r = rx => { r.map_err(|_| CoreError::Cancelled)?; }
                _ = cancel.cancelled() => {
                    let mut st = self.state.lock();
                    if st.waiting.remove(&key).is_none() {
                        // Slot werd net toegekend terwijl we annuleerden: teruggeven.
                        drop(st);
                        self.release(run);
                    }
                    return Err(CoreError::Cancelled);
                }
            }
        }
        let res = self.resources.acquire(
            ResourceKind::GpuSlot,
            &self.name,
            "scheduler",
            Some(run),
            serde_json::json!({"priority": priority}),
        )?;
        self.log.append(
            Event::new(
                EventKind::SchedulerSlotGranted,
                TraceId::new(),
                serde_json::json!({"slot": &*self.name, "priority": priority}),
            )
            .for_run(run),
        )?;
        Ok(SlotPermit { sched: self.clone(), run, _resource: res })
    }

    fn release(&self, run: RunId) {
        let mut st = self.state.lock();
        // Geef direct door aan de belangrijkste wachtende (die nog leeft).
        loop {
            let Some(key) = st.waiting.keys().next().copied() else {
                st.free += 1;
                break;
            };
            if let Some((_, tx)) = st.waiting.remove(&key)
                && tx.send(()).is_ok()
            {
                break;
            }
        }
        drop(st);
        let _ = self.log.append(
            Event::new(EventKind::SchedulerSlotReleased, TraceId::new(), serde_json::json!({"slot": &*self.name}))
                .for_run(run),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};
    use std::time::Duration;

    fn sched() -> Result<SlotScheduler> {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?));
        Ok(SlotScheduler::new("gpu0", 1, log.clone(), ResourceRegistry::new(log)))
    }

    #[tokio::test]
    async fn one_at_a_time_and_chat_first() -> Result<()> {
        let s = sched()?;
        let tok = CancellationToken::new();
        let first = s.acquire(RunId::new(), Priority::P2, &tok).await?;
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut hs = Vec::new();
        for (p, label) in [(Priority::P3, "idle"), (Priority::P2, "ingest"), (Priority::P0, "chat")] {
            let s2 = s.clone();
            let o = order.clone();
            let t = tok.clone();
            hs.push(tokio::spawn(async move {
                let permit = s2.acquire(RunId::new(), p, &t).await;
                o.lock().push(label);
                tokio::time::sleep(Duration::from_millis(5)).await;
                drop(permit);
            }));
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(s.queue_len(), 3);
        drop(first);
        for h in hs {
            h.await.map_err(|e| CoreError::Config(e.to_string()))?;
        }
        assert_eq!(*order.lock(), vec!["chat", "ingest", "idle"]);
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_waiter_leaves_queue_and_slot_is_not_lost() -> Result<()> {
        let s = sched()?;
        let tok = CancellationToken::new();
        let held = s.acquire(RunId::new(), Priority::P0, &tok).await?;
        let wtok = CancellationToken::new();
        let s2 = s.clone();
        let w2 = wtok.clone();
        let h = tokio::spawn(async move { s2.acquire(RunId::new(), Priority::P1, &w2).await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(10)).await;
        wtok.cancel();
        assert!(matches!(h.await.map_err(|e| CoreError::Config(e.to_string()))?, Err(CoreError::Cancelled)));
        drop(held);
        // slot moet weer vrij zijn
        let again = tokio::time::timeout(Duration::from_millis(200), s.acquire(RunId::new(), Priority::P3, &tok)).await;
        assert!(again.is_ok());
        Ok(())
    }
}
