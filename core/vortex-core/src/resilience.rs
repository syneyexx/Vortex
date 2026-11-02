//! Circuit breakers en bulkheads (W04.05). Een falende afhankelijkheid (modelserver, connector)
//! wordt na herhaalde fouten even "open" gezet zodat we niet blijven hameren; een bulkhead begrenst
//! hoeveel van één soort werk tegelijk mag lopen.
// VX:W04.05

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use vortex_types::{Event, EventKind, TraceId};

use crate::error::{CoreError, Result};
use crate::events::EventLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct Inner {
    state: BreakerState,
    failures: u32,
    opened_at: Option<Instant>,
    trial_in_flight: bool,
}

/// Circuit breaker: `threshold` fouten op rij → open voor `cooldown`; daarna één proefaanroep.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    name: Arc<str>,
    threshold: u32,
    cooldown: Duration,
    inner: Arc<Mutex<Inner>>,
    log: Option<EventLog>,
}

impl CircuitBreaker {
    pub fn new(name: &str, threshold: u32, cooldown: Duration, log: Option<EventLog>) -> Self {
        Self {
            name: name.into(),
            threshold: threshold.max(1),
            cooldown,
            inner: Arc::new(Mutex::new(Inner {
                state: BreakerState::Closed,
                failures: 0,
                opened_at: None,
                trial_in_flight: false,
            })),
            log,
        }
    }

    pub fn state(&self) -> BreakerState {
        let mut i = self.inner.lock();
        if i.state == BreakerState::Open && i.opened_at.is_some_and(|t| t.elapsed() >= self.cooldown) {
            i.state = BreakerState::HalfOpen;
            i.trial_in_flight = false;
        }
        i.state
    }

    fn event(&self, kind: EventKind) {
        if let Some(l) = &self.log {
            let _ = l.append(Event::new(kind, TraceId::new(), serde_json::json!({"breaker": &*self.name})));
        }
    }

    /// Mag er een aanroep door? (in half-open: precies één proef).
    pub fn allow(&self) -> bool {
        let st = self.state();
        let mut i = self.inner.lock();
        match st {
            BreakerState::Closed => true,
            BreakerState::Open => false,
            BreakerState::HalfOpen if !i.trial_in_flight => {
                i.trial_in_flight = true;
                true
            }
            BreakerState::HalfOpen => false,
        }
    }

    pub fn success(&self) {
        let was = {
            let mut i = self.inner.lock();
            let was = i.state;
            i.state = BreakerState::Closed;
            i.failures = 0;
            i.trial_in_flight = false;
            was
        };
        if was != BreakerState::Closed {
            self.event(EventKind::CircuitClosed);
        }
    }

    pub fn failure(&self) {
        let opened = {
            let mut i = self.inner.lock();
            i.failures += 1;
            i.trial_in_flight = false;
            if i.state == BreakerState::HalfOpen || i.failures >= self.threshold {
                let newly = i.state != BreakerState::Open;
                i.state = BreakerState::Open;
                i.opened_at = Some(Instant::now());
                newly
            } else {
                false
            }
        };
        if opened {
            tracing::warn!(breaker = %self.name, "circuit open");
            self.event(EventKind::CircuitOpened);
        }
    }

    /// Voer `f` uit door de breaker.
    pub async fn call<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        if !self.allow() {
            return Err(CoreError::QueueFull(format!("circuit '{}' open", self.name)));
        }
        let r = f().await;
        match &r {
            Ok(_) => self.success(),
            Err(CoreError::Cancelled) => {}
            Err(_) => self.failure(),
        }
        r
    }
}

/// Bulkhead: maximaal `n` tegelijk; wachten of direct weigeren.
#[derive(Debug, Clone)]
pub struct Bulkhead {
    name: Arc<str>,
    sem: Arc<Semaphore>,
}

impl Bulkhead {
    pub fn new(name: &str, n: usize) -> Self {
        Self { name: name.into(), sem: Arc::new(Semaphore::new(n.max(1))) }
    }
    pub async fn enter(&self) -> Result<OwnedSemaphorePermit> {
        self.sem.clone().acquire_owned().await.map_err(|_| CoreError::QueueFull(self.name.to_string()))
    }
    pub fn try_enter(&self) -> Result<OwnedSemaphorePermit> {
        self.sem.clone().try_acquire_owned().map_err(|_| CoreError::QueueFull(format!("bulkhead '{}' vol", self.name)))
    }
    pub fn available(&self) -> usize {
        self.sem.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_after_threshold_and_recovers_via_half_open() {
        let b = CircuitBreaker::new("m", 2, Duration::from_millis(30), None);
        let fail = || async { Err::<(), _>(CoreError::Process("x".into())) };
        let _ = b.call(fail).await;
        assert_eq!(b.state(), BreakerState::Closed);
        let _ = b.call(fail).await;
        assert_eq!(b.state(), BreakerState::Open);
        assert!(b.call(|| async { Ok(()) }).await.is_err(), "open = direct weigeren");
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(b.state(), BreakerState::HalfOpen);
        assert!(b.allow());
        assert!(!b.allow(), "maar één proef tegelijk");
        b.success();
        assert_eq!(b.state(), BreakerState::Closed);
    }

    #[tokio::test]
    async fn failed_trial_reopens() {
        let b = CircuitBreaker::new("m", 1, Duration::from_millis(10), None);
        b.failure();
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(b.allow());
        b.failure();
        assert_eq!(b.state(), BreakerState::Open);
    }

    #[tokio::test]
    async fn bulkhead_limits_concurrency() -> Result<()> {
        let h = Bulkhead::new("parse", 2);
        let a = h.enter().await?;
        let _b = h.enter().await?;
        assert!(h.try_enter().is_err());
        drop(a);
        assert!(h.try_enter().is_ok());
        Ok(())
    }
}
