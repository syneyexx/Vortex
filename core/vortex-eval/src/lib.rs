//! # vortex-eval — meten boven geloven (W06)
//! Eval-sets (JSONL), automatische checks, runner met resultaten per versie, vergelijken,
//! retrieval-eval, lekkagecontrole en performance-harnas. Zie `CARD.md`.

pub mod checks;
pub mod leakage;
pub mod metamorphic;
pub mod model;
pub mod perf;
pub mod positional;
pub mod retrieval;
pub mod runner;
pub mod stats;

pub use model::{Check, Counterfactual, EvalTask, RetrievalSet, Suite, load_dir, load_suite};
pub use runner::{Comparison, RunSummary, Runner, Subject, TaskResult, compare, get_run, list_runs, results};
