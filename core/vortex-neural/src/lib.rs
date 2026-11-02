//! Framework-eigen neurale layers (W11): kleine netten (MLP's) op features uit het event log,
//! getraind in Python (PyTorch of numpy), geëxporteerd naar ONNX en hier uitgevoerd met een
//! pure-Rust ONNX-runtime (MLP-operatoren) in microseconden. Versies, schaduwmodus,
//! promotie/rollback op eval en hot-swap via [`LayerRegistry`] (implementeert
//! [`vortex_core::LayerScorer`]).
// VX:W11.04
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod dataset;
pub mod onnx;
pub mod registry;
pub mod spec;

pub use dataset::{DatasetMeta, EvalReport, Example};
pub use onnx::Net;
pub use registry::{LayerRegistry, LayerStatus, Mode};
pub use spec::{LayerKind, LayerSpec, specs};
