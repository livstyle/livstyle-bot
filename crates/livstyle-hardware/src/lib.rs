//! Hardware safety boundary (PRD §10).
//!
//! The [`Supervisor`] owns device leases and the operation state machine; a
//! [`Runner`] performs the device operation and independently enforces fencing
//! tokens and the chip check, so a stale or bypassing caller cannot write to a
//! device even if it reaches the runner directly.

mod approval;
mod ids;
mod lease;
pub mod mock;
mod operation;
mod runner;
mod supervisor;

pub use approval::{Approval, ApprovalError};
pub use ids::{ApprovalId, DeviceId, LeaseId, Millis, OperationId, RunnerId};
pub use lease::{FencingToken, Lease, LeaseError, LeaseManager};
pub use operation::{FlashRegion, OperationIntent, OperationParams, OperationType, TargetChip};
pub use runner::{
    DetectedChip, DispatchRequest, FenceGuard, OperationReceipt, Runner, RunnerOutcome,
    RunnerRejection, chip_check,
};
pub use supervisor::{
    AuditEvent, CancelOutcome, OperationRecord, OperationState, Reconciliation, Supervisor,
    SupervisorError,
};
