use serde::{Deserialize, Serialize};

/// Operation risk level (PRD §10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Read-only checks, log reading, chip identification.
    R0,
    /// Recoverable software changes, sdkconfig edits, ordinary builds.
    R1,
    /// Device deployment, ordinary flash writes, device resets.
    R2,
    /// Erase, boot configuration changes.
    R3,
    /// eFuse, OTP, key burning, irreversible security configuration.
    R4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskPolicy {
    Allow,
    RequireApproval,
    Deny,
}

impl RiskLevel {
    pub fn mvp_policy(self) -> RiskPolicy {
        match self {
            RiskLevel::R0 | RiskLevel::R1 => RiskPolicy::Allow,
            RiskLevel::R2 => RiskPolicy::RequireApproval,
            RiskLevel::R3 | RiskLevel::R4 => RiskPolicy::Deny,
        }
    }
}

/// Verification level of a task result (PRD §11.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VerificationLevel {
    /// Command succeeded.
    V0,
    /// Artifact exists and its hash is valid.
    V1,
    /// Device identity and chip match, with boot evidence.
    V2,
    /// Predefined requirement assertions passed.
    V3,
}

impl VerificationLevel {
    /// V0 and V1 never count as hardware verification.
    pub fn is_hardware_verified(self) -> bool {
        self >= VerificationLevel::V2
    }

    pub fn is_requirement_verified(self) -> bool {
        self == VerificationLevel::V3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp_denies_r3_and_r4() {
        assert_eq!(RiskLevel::R2.mvp_policy(), RiskPolicy::RequireApproval);
        assert_eq!(RiskLevel::R3.mvp_policy(), RiskPolicy::Deny);
        assert_eq!(RiskLevel::R4.mvp_policy(), RiskPolicy::Deny);
    }

    #[test]
    fn command_success_is_not_verification() {
        assert!(!VerificationLevel::V0.is_hardware_verified());
        assert!(!VerificationLevel::V1.is_hardware_verified());
        assert!(VerificationLevel::V2.is_hardware_verified());
        assert!(!VerificationLevel::V2.is_requirement_verified());
        assert!(VerificationLevel::V3.is_requirement_verified());
    }
}
