//! # vortex-core — runtime kern
//!
//! Eigenaar van runs, scheduling, cancellation, resources en configversies. Start bij
//! [`Kernel::open`]; lees `CARD.md` voor de kaart van deze crate.
// VX:W01.01

pub mod budget;
pub mod cache;
pub mod checkpoint;
pub mod config;
pub mod db;
pub mod dirigent;
pub mod error;
pub mod events;
pub mod hddqueue;
pub mod kernel;
pub mod memwatch;
pub mod neural;
pub mod platform;
pub mod process;
pub mod queue;
pub mod recovery;
pub mod resilience;
pub mod resources;
pub mod runs;
pub mod runtimes;
pub mod sampler;
pub mod scheduler;
pub mod scope;
pub mod secrets;
pub mod stall;
pub mod telemetry;

pub use budget::{Budget, BudgetTracker, Deadline, Usage};
pub use cache::{NegativeCache, SingleFlight};
pub use checkpoint::{Checkpoint, Checkpoints, EffectKind};
pub use config::{ConfigManager, ConfigSnapshot};
pub use db::Db;
pub use dirigent::{Conditions, Demand, Dirigent, DirigentConfig, Lease, TaskSpec, Ticket};
pub use error::{CoreError, Result};
pub use events::{Dispatcher, EventConsumer, EventLog};
pub use hddqueue::HddQueue;
pub use kernel::{Kernel, KernelOptions, RunContext};
pub use neural::{LayerScorer, Scorer};
pub use process::{ProcessOutcome, ProcessSpec, ServiceHandle, Supervisor};
pub use queue::{BoundedSender, bounded};
pub use recovery::CrashReport;
pub use resilience::{Bulkhead, CircuitBreaker};
pub use resources::{ResourceGuard, ResourceKind, ResourceRegistry};
pub use runs::{RunRecord, RunSpec, RunStore, TransitionData};
pub use scheduler::{SlotPermit, SlotScheduler};
pub use scope::{DetachedTasks, TaskScope};
pub use secrets::{Secret, SecretBroker};
pub use stall::{RetryBudget, StallDetector};
pub use telemetry::TraceCtx;
