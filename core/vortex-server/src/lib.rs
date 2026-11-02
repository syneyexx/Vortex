//! # vortex-server
//! Het kern-proces: HTTP/WebSocket-API voor de GUI, chat-engine, sessies, live-status.
//! Zie `CARD.md`.

pub mod api;
pub mod chat;
pub mod cognitive;
pub mod eval_api;
pub mod ingest;
pub mod live;
pub mod memory_api;
pub mod neural_api;
pub mod observe_api;
pub mod research_api;
pub mod sessions;
pub mod team_api;
pub mod tools_api;
