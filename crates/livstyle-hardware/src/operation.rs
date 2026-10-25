use std::collections::BTreeMap;

use livstyle_domain::{Interruptibility, RiskLevel, TaskId};
use livstyle_profiles::RevisionPolicy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DeviceId, RunnerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    ReadChipInfo,
    ReadLog,
    Flash,
    Reset,
    EraseRegion,
    EraseFlash,
    WriteEfuse,
}

impl OperationType {
    pub fn risk(self) -> RiskLevel {
        match self {
            OperationType::ReadChipInfo | OperationType::ReadLog => RiskLevel::R0,
            OperationType::Flash | OperationType::Reset => RiskLevel::R2,
            OperationType::EraseRegion | OperationType::EraseFlash => RiskLevel::R3,
            OperationType::WriteEfuse => RiskLevel::R4,
        }
    }

    pub fn interruptibility(self) -> Interruptibility {
        match self {
            OperationType::ReadChipInfo | OperationType::ReadLog => Interruptibility::Free,
            _ => Interruptibility::NonInterruptible,
        }
    }

    pub fn writes_device(self) -> bool {
        !matches!(self, OperationType::ReadChipInfo | OperationType::ReadLog)
    }
}

/// Chip the artifact was built for, with the revision range from its chip profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetChip {
    pub chip: String,
    pub revisions: RevisionPolicy,
    pub chip_profile_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashRegion {
    pub offset: u32,
    pub sha256: String,
}

/// Every field that an approval must bind to. Any change yields a new digest
/// and therefore invalidates prior approvals (PRD §10.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationParams {
    pub operation_type: OperationType,
    pub device_id: DeviceId,
    pub runner_id: RunnerId,
    pub target: Option<TargetChip>,
    pub board_profile_id: Option<String>,
    pub artifact_sha256: Option<String>,
    #[serde(default)]
    pub regions: Vec<FlashRegion>,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

impl OperationParams {
    /// SHA-256 over the canonical JSON encoding (struct field order is fixed,
    /// maps are ordered).
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("params serialize");
        hex::encode(Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationIntent {
    pub task_id: TaskId,
    pub params: OperationParams,
}

impl OperationIntent {
    pub fn risk(&self) -> RiskLevel {
        self.params.operation_type.risk()
    }
}
