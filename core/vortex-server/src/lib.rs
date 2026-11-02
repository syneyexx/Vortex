//! # vortex-server
//! Het kern-proces: HTTP/WebSocket-API voor de GUI, chat-engine, sessies, live-status.
//! Zie `CARD.md`.

pub mod api;
pub mod chat;
pub mod eval_api;
pub mod live;
pub mod memory_api;
pub mod observe_api;
pub mod sessions;
