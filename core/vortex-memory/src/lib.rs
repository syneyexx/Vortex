//! # vortex-memory — het geheugen van Vortex (W05)
//!
//! SQLite = bron van waarheid (herinneringen, FTS5, graph, claims, herkomst, conflicten, vectoren);
//! RAM-index en metadata zijn afgeleid. Start bij [`MemoryStore::open`]. Zie `CARD.md`.

pub mod chunk;
pub mod embed;
pub mod error;
pub mod features;
pub mod llm;
pub mod maintenance;
pub mod policy;
pub mod retrieve;
pub mod store;
pub mod types;
pub mod vector;

pub use embed::{BatchingEmbedder, Embedder, HashEmbedder};
pub use error::{MemError, Result};
pub use llm::{HeuristicLlm, MemLlm};
pub use retrieve::{Query, Reranker};
pub use store::{MemConfig, MemoryStore};
pub use types::*;
