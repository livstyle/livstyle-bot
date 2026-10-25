//! Cursor Agent Runtime over the official SDK bridge `sdk.v1` protocol
//! (PRD §9, ADR-003).
//!
//! Cursor is modelled as an external agent runtime, not a model provider:
//! livstyle creates agents, sends turns, observes and cancels runs.

mod bridge;
mod client;
mod errors;
mod ready;
mod stream;
mod transport;

pub use bridge::{Bridge, BridgeConfig, PINNED_BRIDGE_VERSION, locate_bridge};
pub use client::{
    account_status_line, AgentHandle, CursorClient, ModelInfo, UserInfo, VersionInfo, runtime_caps,
};
pub use errors::{CursorError, SdkErrorCode, SdkErrorDetails};
pub use ready::{ReadyInfo, parse_ready_line};
pub use stream::{RunEvent, RunStatus, RunStream, RunSummary};
pub use transport::Endpoint;
