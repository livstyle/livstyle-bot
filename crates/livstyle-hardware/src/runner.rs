use std::collections::HashMap;

use livstyle_domain::TaskId;
use livstyle_profiles::ChipRevision;
use serde::{Deserialize, Serialize};

use crate::{DeviceId, FencingToken, OperationId, OperationParams, RunnerId, TargetChip};

/// What the runner reads from the device itself (esptool `chip_id` etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedChip {
    pub chip: String,
    pub revision: ChipRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchRequest {
    pub operation_id: OperationId,
    pub task_id: TaskId,
    pub fencing_token: FencingToken,
    pub parameters_digest: String,
    pub params: OperationParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationReceipt {
    pub operation_id: OperationId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub command_summary: String,
    pub exit_status: Option<i32>,
    pub output_summary: String,
    pub detected_chip: Option<DetectedChip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunnerRejection {
    StaleFencingToken {
        presented: FencingToken,
        highest_seen: FencingToken,
    },
    WrongRunner {
        expected: RunnerId,
    },
    DeviceNotFound,
    ChipMismatch {
        expected: String,
        detected: String,
    },
    RevisionOutOfRange {
        chip: String,
        detected: ChipRevision,
    },
    MissingTarget,
    /// The files on disk are not the artifact the approval was granted for.
    ArtifactMismatch {
        expected: String,
        actual: String,
    },
    /// This runner does not implement the operation type.
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunnerOutcome {
    Completed(OperationReceipt),
    Failed(OperationReceipt),
    /// Refused before touching the device.
    Rejected(RunnerRejection),
    /// The runner cannot tell whether the operation took effect
    /// (transport lost mid-write, timeout, crash).
    OutcomeUnknown {
        detail: String,
    },
}

pub trait Runner {
    fn runner_id(&self) -> &RunnerId;
    fn execute(&mut self, request: &DispatchRequest) -> RunnerOutcome;
}

/// Runner-side fencing (PRD §10.1): remembers the highest token seen per
/// device and refuses anything lower, independent of the supervisor.
#[derive(Debug, Default)]
pub struct FenceGuard {
    highest: HashMap<DeviceId, FencingToken>,
}

impl FenceGuard {
    pub fn admit(&mut self, device: &DeviceId, token: FencingToken) -> Result<(), RunnerRejection> {
        let highest = self.highest.entry(device.clone()).or_insert(token);
        if token < *highest {
            return Err(RunnerRejection::StaleFencingToken {
                presented: token,
                highest_seen: *highest,
            });
        }
        *highest = token;
        Ok(())
    }
}

/// The ChipCheck step (PRD §11.3): the detected chip and revision must match
/// the artifact's target before any write.
pub fn chip_check(
    target: Option<&TargetChip>,
    detected: &DetectedChip,
) -> Result<(), RunnerRejection> {
    let target = target.ok_or(RunnerRejection::MissingTarget)?;
    if target.chip != detected.chip {
        return Err(RunnerRejection::ChipMismatch {
            expected: target.chip.clone(),
            detected: detected.chip.clone(),
        });
    }
    if !target.revisions.allows(detected.revision) {
        return Err(RunnerRejection::RevisionOutOfRange {
            chip: detected.chip.clone(),
            detected: detected.revision,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_rejects_lower_tokens_only() {
        let dev = DeviceId::new("d");
        let mut fence = FenceGuard::default();
        fence.admit(&dev, FencingToken(2)).unwrap();
        fence.admit(&dev, FencingToken(2)).unwrap();
        assert!(fence.admit(&dev, FencingToken(1)).is_err());
        fence.admit(&dev, FencingToken(3)).unwrap();
    }
}
