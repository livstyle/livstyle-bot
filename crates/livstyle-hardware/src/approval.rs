use livstyle_domain::RiskLevel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ApprovalId, Millis, OperationType};

/// A user's grant for exactly one parameter digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub approval_id: ApprovalId,
    pub operation_type: OperationType,
    pub risk: RiskLevel,
    pub parameters_digest: String,
    pub approved_by: String,
    pub granted_at: Millis,
    pub expires_at: Millis,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalError {
    #[error("approval is bound to a different parameter digest")]
    DigestMismatch,
    #[error("approval covers {approved:?}, operation is {requested:?}")]
    OperationMismatch {
        approved: OperationType,
        requested: OperationType,
    },
    #[error("approval expired at {expires_at}")]
    Expired { expires_at: Millis },
}

impl Approval {
    pub fn check(
        &self,
        operation_type: OperationType,
        digest: &str,
        now: Millis,
    ) -> Result<(), ApprovalError> {
        if self.operation_type != operation_type || self.risk != operation_type.risk() {
            return Err(ApprovalError::OperationMismatch {
                approved: self.operation_type,
                requested: operation_type,
            });
        }
        if self.parameters_digest != digest {
            return Err(ApprovalError::DigestMismatch);
        }
        if self.expires_at <= now {
            return Err(ApprovalError::Expired {
                expires_at: self.expires_at,
            });
        }
        Ok(())
    }
}
