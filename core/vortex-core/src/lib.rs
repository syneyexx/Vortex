//! # vortex-core — runtime kern
//!
//! Eigenaar van runs, scheduling, cancellation, resources en configversies. Start bij
//! [`Kernel::open`]; lees `CARD.md` voor de kaart van deze crate.
// VX:W01.01

pub mod budget;
pub mod checkpoint;
pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod kernel;
pub mod process;
pub mod queue;
pub mod recovery;
pub mod resources;
pub mod runs;
pub mod scheduler;
pub mod scope;
pub mod secrets;
pub mod telemetry;

pub use budget::{Budget, BudgetTracker, Deadline, Usage};
pub use checkpoint::{Checkpoint, Checkpoints, EffectKind};
pub use config::{ConfigManager, ConfigSnapshot};
pub use db::Db;
pub use error::{CoreError, Result};
pub use events::{Dispatcher, EventConsumer, EventLog};
pub use kernel::{Kernel, KernelOptions, RunContext};
pub use process::{ProcessOutcome, ProcessSpec, ServiceHandle, Supervisor};
pub use queue::{BoundedSender, bounded};
pub use recovery::CrashReport;
pub use resources::{ResourceGuard, ResourceKind, ResourceRegistry};
pub use runs::{RunRecord, RunSpec, RunStore, TransitionData};
pub use scheduler::{SlotPermit, SlotScheduler};
pub use scope::{DetachedTasks, TaskScope};
pub use secrets::{Secret, SecretBroker};
pub use telemetry::TraceCtx;
