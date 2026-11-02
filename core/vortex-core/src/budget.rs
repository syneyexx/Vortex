//! Deadlines en budgetten (stappen, tokens, tijd). Subtaken erven het resterende budget;
//! verbruik van een kind telt ook bij de ouder.
// VX:W01.07

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::error::{CoreError, Result};

/// Limieten van een taak. `None` = onbeperkt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,
}

/// Verbruik.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    pub steps: u64,
    pub tokens: u64,
    pub elapsed_ms: u64,
}

/// Eén eindtijd voor een taak en al haar subtaken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(Option<Instant>);

impl Deadline {
    pub const NONE: Deadline = Deadline(None);

    pub fn after(d: Duration) -> Self {
        Self(Some(Instant::now() + d))
    }
    pub fn at(i: Instant) -> Self {
        Self(Some(i))
    }
    pub fn instant(&self) -> Option<Instant> {
        self.0
    }
    /// Deadline voor een subtaak: nooit later dan die van de ouder.
    pub fn child(&self, own: Option<Duration>) -> Deadline {
        let own = own.map(|d| Instant::now() + d);
        Deadline(match (self.0, own) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        })
    }
    pub fn remaining(&self) -> Option<Duration> {
        self.0.map(|i| i.saturating_duration_since(Instant::now()))
    }
    pub fn expired(&self) -> bool {
        self.0.is_some_and(|i| Instant::now() >= i)
    }
    /// Wacht tot de deadline (nooit als er geen is).
    pub async fn wait(&self) {
        match self.0 {
            Some(i) => tokio::time::sleep_until(i).await,
            None => std::future::pending().await,
        }
    }
}

#[derive(Debug)]
struct Inner {
    budget: Budget,
    steps: AtomicU64,
    tokens: AtomicU64,
    started: Instant,
    deadline: Deadline,
    parent: Option<BudgetTracker>,
}

/// Bewaakt budget + deadline van één (sub)taak. Goedkoop te klonen.
#[derive(Debug, Clone)]
pub struct BudgetTracker(Arc<Inner>);

impl BudgetTracker {
    pub fn new(budget: Budget, deadline: Deadline) -> Self {
        let deadline = deadline.child(budget.max_time_ms.map(Duration::from_millis));
        Self(Arc::new(Inner {
            budget,
            steps: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            started: Instant::now(),
            deadline,
            parent: None,
        }))
    }

    /// Subtaak met eigen (optionele) limieten; erft resterend budget en deadline van de ouder.
    pub fn child(&self, own: Budget) -> Self {
        let rem = self.remaining();
        let min = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        };
        let budget = Budget {
            max_steps: min(own.max_steps, rem.max_steps),
            max_tokens: min(own.max_tokens, rem.max_tokens),
            max_time_ms: min(own.max_time_ms, rem.max_time_ms),
        };
        let deadline = self.0.deadline.child(budget.max_time_ms.map(Duration::from_millis));
        Self(Arc::new(Inner {
            budget,
            steps: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            started: Instant::now(),
            deadline,
            parent: Some(self.clone()),
        }))
    }

    pub fn deadline(&self) -> Deadline {
        self.0.deadline
    }

    /// Resterend budget (voor kinderen).
    pub fn remaining(&self) -> Budget {
        let b = &self.0.budget;
        Budget {
            max_steps: b.max_steps.map(|m| m.saturating_sub(self.0.steps.load(Ordering::Relaxed))),
            max_tokens: b.max_tokens.map(|m| m.saturating_sub(self.0.tokens.load(Ordering::Relaxed))),
            max_time_ms: self.0.deadline.remaining().map(|d| d.as_millis() as u64),
        }
    }

    pub fn usage(&self) -> Usage {
        Usage {
            steps: self.0.steps.load(Ordering::Relaxed),
            tokens: self.0.tokens.load(Ordering::Relaxed),
            elapsed_ms: self.0.started.elapsed().as_millis() as u64,
        }
    }

    /// Controleer deadline en limieten zonder te verbruiken.
    pub fn check(&self) -> Result<()> {
        if self.0.deadline.expired() {
            return Err(CoreError::DeadlineExceeded);
        }
        let b = &self.0.budget;
        if b.max_steps.is_some_and(|m| self.0.steps.load(Ordering::Relaxed) > m) {
            return Err(CoreError::BudgetExceeded("stappen".into()));
        }
        if b.max_tokens.is_some_and(|m| self.0.tokens.load(Ordering::Relaxed) > m) {
            return Err(CoreError::BudgetExceeded("tokens".into()));
        }
        match &self.0.parent {
            Some(p) => p.check(),
            None => Ok(()),
        }
    }

    /// Eén stap verbruiken (ook bij alle ouders).
    pub fn charge_step(&self) -> Result<()> {
        self.0.steps.fetch_add(1, Ordering::Relaxed);
        if let Some(p) = &self.0.parent {
            p.charge_step()?;
        }
        self.check()
    }

    /// Tokens verbruiken (ook bij alle ouders).
    pub fn charge_tokens(&self, n: u64) -> Result<()> {
        self.0.tokens.fetch_add(n, Ordering::Relaxed);
        if let Some(p) = &self.0.parent {
            p.charge_tokens(n)?;
        }
        self.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_budget_is_enforced() {
        let b = BudgetTracker::new(Budget { max_steps: Some(2), ..Default::default() }, Deadline::NONE);
        assert!(b.charge_step().is_ok());
        assert!(b.charge_step().is_ok());
        assert!(matches!(b.charge_step(), Err(CoreError::BudgetExceeded(_))));
    }

    #[test]
    fn child_inherits_remaining_and_charges_parent() {
        let p = BudgetTracker::new(Budget { max_tokens: Some(100), ..Default::default() }, Deadline::NONE);
        p.charge_tokens(60).ok();
        let c = p.child(Budget { max_tokens: Some(1000), ..Default::default() });
        assert_eq!(c.remaining().max_tokens, Some(40));
        assert!(c.charge_tokens(30).is_ok());
        assert_eq!(p.usage().tokens, 90);
        assert!(c.charge_tokens(20).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_propagates_to_child() {
        let p = BudgetTracker::new(Budget::default(), Deadline::after(Duration::from_millis(100)));
        let c = p.child(Budget { max_time_ms: Some(10_000), ..Default::default() });
        assert!(c.deadline().remaining() <= Some(Duration::from_millis(100)));
        tokio::time::advance(Duration::from_millis(101)).await;
        assert!(matches!(c.check(), Err(CoreError::DeadlineExceeded)));
    }

    #[test]
    fn child_deadline_never_later_than_parent() {
        let d = Deadline::after(Duration::from_secs(1));
        let c = d.child(Some(Duration::from_secs(10)));
        assert!(c.instant() <= d.instant());
        let c2 = Deadline::NONE.child(Some(Duration::from_secs(1)));
        assert!(c2.instant().is_some());
    }
}
