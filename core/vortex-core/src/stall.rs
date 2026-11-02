//! Stall/livelock- en retry-amplification-detectie (W04.06).
//!
//! Signalen per run: dezelfde actie herhaald zonder nieuw resultaat, dezelfde fout herhaald, plan
//! herschreven zonder uitvoering, en geen nieuw resultaat binnen een tijdvenster. Retry-
//! amplification: totaal aantal pogingen in een trace loopt uit de hand (retries op meerdere lagen).
// VX:W04.06

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use vortex_types::{Event, EventKind, RunId, TraceId};

use crate::config::fnv1a64;
use crate::events::EventLog;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum StallSignal {
    RepeatedAction { action: String, times: u32 },
    RepeatedError { error: String, times: u32 },
    PlanChurn { rewrites: u32 },
    NoProgress { secs: u64 },
}

#[derive(Debug, Clone)]
pub struct StallLimits {
    pub same_action: u32,
    pub same_error: u32,
    pub plan_rewrites: u32,
    pub no_progress: Duration,
}

impl Default for StallLimits {
    fn default() -> Self {
        Self { same_action: 3, same_error: 3, plan_rewrites: 3, no_progress: Duration::from_secs(120) }
    }
}

/// Houdt per run bij wat er gebeurt.
#[derive(Debug)]
pub struct StallDetector {
    run: RunId,
    trace: TraceId,
    limits: StallLimits,
    last_action: Option<u64>,
    action_repeats: u32,
    last_error: Option<u64>,
    error_repeats: u32,
    rewrites_since_exec: u32,
    last_progress: Instant,
    seen_results: std::collections::HashSet<u64>,
    log: Option<EventLog>,
}

impl StallDetector {
    pub fn new(run: RunId, trace: TraceId, limits: StallLimits, log: Option<EventLog>) -> Self {
        Self {
            run,
            trace,
            limits,
            last_action: None,
            action_repeats: 0,
            last_error: None,
            error_repeats: 0,
            rewrites_since_exec: 0,
            last_progress: Instant::now(),
            seen_results: Default::default(),
            log,
        }
    }

    fn emit(&self, s: &StallSignal) {
        if let Some(l) = &self.log {
            let _ = l.append(
                Event::new(EventKind::StallDetected, self.trace, serde_json::to_value(s).unwrap_or_default())
                    .for_run(self.run),
            );
        }
    }

    fn check(&self, s: Option<StallSignal>) -> Option<StallSignal> {
        if let Some(sig) = &s {
            self.emit(sig);
        }
        s
    }

    /// Een actie (tool + argumenten) en het resultaat ervan.
    pub fn action(&mut self, name: &str, args: &str, result: &str) -> Option<StallSignal> {
        let a = fnv1a64(format!("{name}\0{args}").as_bytes());
        let r = fnv1a64(result.as_bytes());
        self.rewrites_since_exec = 0;
        let new_result = self.seen_results.insert(r);
        if new_result {
            self.last_progress = Instant::now();
        }
        if self.last_action == Some(a) && !new_result {
            self.action_repeats += 1;
        } else {
            self.action_repeats = 1;
        }
        self.last_action = Some(a);
        let s = (self.action_repeats >= self.limits.same_action)
            .then(|| StallSignal::RepeatedAction { action: name.into(), times: self.action_repeats });
        self.check(s)
    }

    pub fn error(&mut self, err: &str) -> Option<StallSignal> {
        let e = fnv1a64(err.as_bytes());
        if self.last_error == Some(e) {
            self.error_repeats += 1;
        } else {
            self.error_repeats = 1;
        }
        self.last_error = Some(e);
        let s = (self.error_repeats >= self.limits.same_error)
            .then(|| StallSignal::RepeatedError { error: err.chars().take(200).collect(), times: self.error_repeats });
        self.check(s)
    }

    pub fn plan_rewritten(&mut self) -> Option<StallSignal> {
        self.rewrites_since_exec += 1;
        let s = (self.rewrites_since_exec >= self.limits.plan_rewrites)
            .then_some(StallSignal::PlanChurn { rewrites: self.rewrites_since_exec });
        self.check(s)
    }

    /// Periodiek aanroepen.
    pub fn tick(&mut self) -> Option<StallSignal> {
        let el = self.last_progress.elapsed();
        let s = (el >= self.limits.no_progress).then_some(StallSignal::NoProgress { secs: el.as_secs() });
        if s.is_some() {
            self.last_progress = Instant::now(); // niet elke tick opnieuw melden
        }
        self.check(s)
    }
}

/// Telt pogingen per trace; meldt als het totaal over lagen heen uit de hand loopt.
#[derive(Debug)]
pub struct RetryBudget {
    max_per_trace: u32,
    attempts: HashMap<TraceId, u32>,
    log: Option<EventLog>,
}

impl RetryBudget {
    pub fn new(max_per_trace: u32, log: Option<EventLog>) -> Self {
        Self { max_per_trace, attempts: HashMap::new(), log }
    }

    /// Registreer een retry. `false` = budget op (niet opnieuw proberen) + event.
    pub fn retry(&mut self, trace: TraceId, layer: &str) -> bool {
        let n = self.attempts.entry(trace).or_insert(0);
        *n += 1;
        if *n == self.max_per_trace + 1
            && let Some(l) = &self.log
        {
            let _ = l.append(Event::new(
                EventKind::RetryAmplified,
                trace,
                serde_json::json!({"attempts": *n, "layer": layer}),
            ));
        }
        *n <= self.max_per_trace
    }

    pub fn finish(&mut self, trace: TraceId) {
        self.attempts.remove(&trace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det() -> StallDetector {
        StallDetector::new(
            RunId::new(),
            TraceId::new(),
            StallLimits { no_progress: Duration::from_millis(20), ..Default::default() },
            None,
        )
    }

    #[test]
    fn same_action_same_result_is_livelock() {
        let mut d = det();
        assert!(d.action("search", "q=x", "geen").is_none());
        assert!(d.action("search", "q=x", "geen").is_none());
        assert!(matches!(d.action("search", "q=x", "geen"), Some(StallSignal::RepeatedAction { times: 3, .. })));
    }

    #[test]
    fn same_action_new_results_is_progress() {
        let mut d = det();
        for i in 0..5 {
            assert!(d.action("read_page", "p", &format!("pagina {i}")).is_none());
        }
    }

    #[test]
    fn repeated_error_and_plan_churn() {
        let mut d = det();
        d.error("timeout");
        d.error("timeout");
        assert!(d.error("timeout").is_some());
        d.plan_rewritten();
        d.plan_rewritten();
        assert!(matches!(d.plan_rewritten(), Some(StallSignal::PlanChurn { rewrites: 3 })));
        d.action("x", "", "r");
        assert!(d.plan_rewritten().is_none(), "uitvoering reset de teller");
    }

    #[test]
    fn no_progress_after_window() {
        let mut d = det();
        std::thread::sleep(Duration::from_millis(25));
        assert!(matches!(d.tick(), Some(StallSignal::NoProgress { .. })));
        assert!(d.tick().is_none());
    }

    #[test]
    fn retry_budget_caps_amplification() {
        let mut b = RetryBudget::new(3, None);
        let t = TraceId::new();
        assert!(b.retry(t, "tool"));
        assert!(b.retry(t, "agent"));
        assert!(b.retry(t, "model"));
        assert!(!b.retry(t, "tool"), "4e poging over alle lagen = stop");
        b.finish(t);
        assert!(b.retry(t, "tool"));
    }
}
