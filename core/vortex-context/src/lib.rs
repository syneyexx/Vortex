//! # vortex-context
//! Context assembly (W07) en de koppeling geheugen ⇄ modellen ([`adapters`]). Zie `CARD.md`.

pub mod adapters;
pub mod assembler;

pub use assembler::{
    Assembled, AssemblyInput, AssemblyReport, Budget, MemItem, ToolBrief, Turn, assemble, assemble_for_role,
};
