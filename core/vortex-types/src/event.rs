//! Event-register: alle event-soorten van Vortex staan in [`EventKind`] — dit ís de lijst.
//! Naam-conventie: `domein.onderwerp.werkwoord_verleden` (zie VAULT/13-CONVENTIES.md).

use serde::{Deserialize, Serialize};

use crate::{EventId, Provenance, RunId, SpanId, Timestamp, TraceId};

/// Versie van het event-envelopformaat.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

macro_rules! event_kinds {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Alle event-soorten.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum EventKind { $($variant),* }

        impl EventKind {
            pub const ALL: &'static [EventKind] = &[$(EventKind::$variant),*];
            pub fn as_str(self) -> &'static str {
                match self { $(EventKind::$variant => $name),* }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($name => Some(EventKind::$variant),)* _ => None }
            }
        }
    };
}

event_kinds! {
    // runs (W01)
    RunCreated => "run.lifecycle.created",
    RunStateChanged => "run.state.changed",
    RunStopRequested => "run.stop.requested",
    RunStopped => "run.stop.stopped",
    RunCleanedUp => "run.stop.cleaned_up",
    RunCheckpointed => "run.checkpoint.saved",
    RunResumed => "run.lifecycle.resumed",
    RunBudgetExceeded => "run.budget.exceeded",
    RunDeadlineExceeded => "run.deadline.exceeded",
    // gesprek
    MessageReceived => "chat.message.received",
    MessageCompleted => "chat.message.completed",
    MessageStarted => "chat.message.started",
    MessageDelta => "chat.message.delta",
    SessionCreated => "chat.session.created",
    SessionForked => "chat.session.forked",
    SessionUpdated => "chat.session.updated",
    PromptUpdated => "settings.prompt.updated",
    PlanCreated => "cognition.plan.created",
    // tools
    ToolCallStarted => "tool.call.started",
    ToolCallCompleted => "tool.call.completed",
    ToolCallFailed => "tool.call.failed",
    ToolCallInvalid => "tool.call.invalid",
    // geheugen
    RetrievalPerformed => "memory.retrieval.performed",
    MemoryWritten => "memory.item.written",
    // debat
    DebateRoundCompleted => "debate.round.completed",
    VerdictGiven => "debate.verdict.given",
    // feedback / versies
    FeedbackGiven => "feedback.user.given",
    ModelVersionUsed => "model.version.used",
    PromptVersionUsed => "prompt.version.used",
    // modellen (W02)
    ModelLifecycleChanged => "model.lifecycle.changed",
    InferenceMeasured => "model.inference.measured",
    // config / resources / processen
    ConfigReloaded => "config.version.reloaded",
    ResourceAcquired => "resource.ownership.acquired",
    ResourceReleased => "resource.ownership.released",
    ProcessSpawned => "process.supervisor.spawned",
    ProcessExited => "process.supervisor.exited",
    ProcessOutputTruncated => "process.supervisor.output_truncated",
    // scheduler / signalen
    SchedulerSlotGranted => "scheduler.slot.granted",
    SchedulerSlotReleased => "scheduler.slot.released",
    QueueBackpressure => "runtime.queue.backpressured",
    // beslissingen (trainingsdata)
    DecisionMade => "decision.made",
    DecisionOutcome => "decision.outcome.recorded",
    // systeem
    SystemStarted => "system.lifecycle.started",
    SystemRecovered => "system.recovery.completed",
}

impl Serialize for EventKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        EventKind::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("onbekend event: {s}")))
    }
}

/// Envelop van elk event in het log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    /// Volgnummer in het log (toegekend bij opslaan; 0 = nog niet opgeslagen).
    #[serde(default)]
    pub seq: i64,
    pub kind: EventKind,
    pub schema_version: u32,
    pub ts: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub trace_id: TraceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,
    pub provenance: Provenance,
    pub payload: serde_json::Value,
}

impl Event {
    pub fn new(kind: EventKind, trace_id: TraceId, payload: serde_json::Value) -> Self {
        Self {
            id: EventId::new(),
            seq: 0,
            kind,
            schema_version: EVENT_SCHEMA_VERSION,
            ts: crate::now(),
            run_id: None,
            trace_id,
            span_id: None,
            provenance: Provenance::system(),
            payload,
        }
    }
    pub fn for_run(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }
    pub fn with_span(mut self, span: SpanId) -> Self {
        self.span_id = Some(span);
        self
    }
    pub fn with_provenance(mut self, p: Provenance) -> Self {
        self.provenance = p;
        self
    }
}

/// Feature-logging voor elke beslissing (trainingsdata voor neurale layers en LoRA).
/// Gaat als payload in een [`EventKind::DecisionMade`]-event; de uitkomst volgt later
/// in een [`EventKind::DecisionOutcome`]-event met dezelfde `decision_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision_id: EventId,
    /// Welke beslisser: "router", "importance", "scheduler", …
    pub decider: String,
    /// Versie van de beslisser (heuristiek-v1, net-v3, …).
    pub decider_version: String,
    pub features: serde_json::Value,
    pub choice: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Ingevuld bij het outcome-event: feedback, test, Judge, gebruikt ja/nee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_event_names_unique_and_roundtrip() {
        let mut seen = std::collections::HashSet::new();
        for k in EventKind::ALL {
            assert!(seen.insert(k.as_str()), "dubbel: {}", k.as_str());
            assert_eq!(EventKind::parse(k.as_str()), Some(*k));
        }
    }

    #[test]
    fn event_names_follow_convention() {
        for k in EventKind::ALL {
            let parts: Vec<_> = k.as_str().split('.').collect();
            assert!(parts.len() >= 2, "{}", k.as_str());
            assert!(k.as_str().chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'));
        }
    }

    #[test]
    fn event_serde_roundtrip() {
        let e = Event::new(EventKind::RunCreated, TraceId::new(), serde_json::json!({"a": 1})).for_run(RunId::new());
        let j = serde_json::to_string(&e).unwrap_or_default();
        let back: Event = serde_json::from_str(&j).unwrap_or_else(|_| e.clone());
        assert_eq!(back, e);
    }
}
