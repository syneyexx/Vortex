//! # vortex-cognition — het deel van het brein in het framework (W07)
//! Cyclus (8 fasen), Global Workspace + doelstapel, router, metacognitie, planning
//! (plan-and-execute, reflexion, ReAct), salience, leeragenda, gebruikersmodel. Zie `CARD.md`.

pub mod agenda;
pub mod cycle;
pub mod llm;
pub mod metacog;
pub mod planning;
pub mod router;
pub mod salience;
pub mod selfmodel;
pub mod usermodel;
pub mod workspace;

pub use agenda::Agenda;
pub use cycle::{Cycle, Phase};
pub use llm::{CogLlm, GenOpts};
pub use metacog::{Path, PlanStyle, Prediction, Signals, Strategy, decide, switch_on};
pub use router::{Route, TaskType, route};
pub use selfmodel::SelfModel;
pub use usermodel::UserProfile;
pub use workspace::Workspace;
