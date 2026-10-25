//! Octos UI protocol (OUP) client for Spike 2.
//!
//! Speaks JSON-RPC 2.0 over the `/api/ui-protocol/ws` WebSocket that
//! `octos serve` exposes. Loopback only: the bearer token must not leave
//! the machine. Turns themselves are driven by Octos; this client records
//! what the server accepts and what it refuses (for example a session with
//! no LLM provider).

mod client;
mod ws;

pub use client::{
    FEATURE_SESSION_HYDRATE, FEATURE_TASK_CONTROL, FEATURE_TURN_STEER_DROPPED, Hello, OctosClient,
    OctosError, ServerCaps,
};
