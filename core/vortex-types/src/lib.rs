//! Gedeelde typen voor heel Vortex. Geen logica, geen I/O.
//! Elke andere crate hangt hiervan af; deze crate hangt van geen enkele interne crate af.
// VX:W00.10

pub mod error;
pub mod event;
pub mod ids;
pub mod provenance;
pub mod run;

pub use error::{ErrorKind, VortexError};
pub use event::{DecisionRecord, EVENT_SCHEMA_VERSION, Event, EventKind};
pub use ids::*;
pub use provenance::{Provenance, SourceKind};
pub use run::{Priority, RunKind, RunState, StopStatus};

/// Tijdstip, altijd UTC in opslag.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// Huidige tijd (UTC).
pub fn now() -> Timestamp {
    chrono::Utc::now()
}
