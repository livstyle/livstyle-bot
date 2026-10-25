use std::collections::HashMap;

use livstyle_domain::TaskId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DeviceId, LeaseId, Millis, RunnerId};

/// Monotonically increasing per device. A higher token supersedes every lower one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FencingToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub lease_id: LeaseId,
    pub device_id: DeviceId,
    pub task_id: TaskId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub expires_at: Millis,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LeaseError {
    #[error("device {device} is leased to task {holder} until {expires_at}")]
    Held {
        device: DeviceId,
        holder: TaskId,
        expires_at: Millis,
    },
    #[error("no active lease on device {0}")]
    NotHeld(DeviceId),
    #[error("lease expired at {expires_at}")]
    Expired { expires_at: Millis },
    #[error("fencing token {presented:?} superseded by {current:?}")]
    Superseded {
        presented: FencingToken,
        current: FencingToken,
    },
    #[error("lease belongs to another task or runner")]
    NotOwner,
}

#[derive(Debug, Default)]
pub struct LeaseManager {
    active: HashMap<DeviceId, Lease>,
    last_token: HashMap<DeviceId, u64>,
    seq: u64,
}

impl LeaseManager {
    pub fn acquire(
        &mut self,
        device: &DeviceId,
        task: &TaskId,
        runner: &RunnerId,
        ttl: Millis,
        now: Millis,
    ) -> Result<Lease, LeaseError> {
        if let Some(current) = self.active.get(device) {
            if current.expires_at > now {
                return Err(LeaseError::Held {
                    device: device.clone(),
                    holder: current.task_id.clone(),
                    expires_at: current.expires_at,
                });
            }
        }
        let token = self.last_token.entry(device.clone()).or_default();
        *token += 1;
        self.seq += 1;
        let lease = Lease {
            lease_id: LeaseId::new(format!("lease-{}", self.seq)),
            device_id: device.clone(),
            task_id: task.clone(),
            runner_id: runner.clone(),
            fencing_token: FencingToken(*token),
            expires_at: now.saturating_add(ttl),
        };
        self.active.insert(device.clone(), lease.clone());
        Ok(lease)
    }

    /// Checks that `lease` is the current, unexpired lease for its device.
    pub fn validate(&self, lease: &Lease, now: Millis) -> Result<(), LeaseError> {
        let current = self
            .active
            .get(&lease.device_id)
            .ok_or_else(|| LeaseError::NotHeld(lease.device_id.clone()))?;
        if current.fencing_token != lease.fencing_token {
            return Err(LeaseError::Superseded {
                presented: lease.fencing_token,
                current: current.fencing_token,
            });
        }
        if current.task_id != lease.task_id || current.runner_id != lease.runner_id {
            return Err(LeaseError::NotOwner);
        }
        if current.expires_at <= now {
            return Err(LeaseError::Expired {
                expires_at: current.expires_at,
            });
        }
        Ok(())
    }

    pub fn renew(&mut self, lease: &Lease, ttl: Millis, now: Millis) -> Result<Lease, LeaseError> {
        self.validate(lease, now)?;
        let current = self.active.get_mut(&lease.device_id).expect("validated");
        current.expires_at = now.saturating_add(ttl);
        Ok(current.clone())
    }

    /// Releasing an expired or superseded lease is a no-op error; it must not
    /// free a lease that now belongs to someone else.
    pub fn release(&mut self, lease: &Lease) -> Result<(), LeaseError> {
        let current = self
            .active
            .get(&lease.device_id)
            .ok_or_else(|| LeaseError::NotHeld(lease.device_id.clone()))?;
        if current.fencing_token != lease.fencing_token {
            return Err(LeaseError::Superseded {
                presented: lease.fencing_token,
                current: current.fencing_token,
            });
        }
        self.active.remove(&lease.device_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (DeviceId, TaskId, TaskId, RunnerId) {
        (
            DeviceId::new("dev"),
            TaskId::new("a"),
            TaskId::new("b"),
            RunnerId::new("r"),
        )
    }

    #[test]
    fn exclusive_until_expiry_then_token_increases() {
        let (dev, a, b, r) = ids();
        let mut m = LeaseManager::default();
        let la = m.acquire(&dev, &a, &r, 100, 0).unwrap();
        assert!(matches!(
            m.acquire(&dev, &b, &r, 100, 50),
            Err(LeaseError::Held { .. })
        ));
        let lb = m.acquire(&dev, &b, &r, 100, 100).unwrap();
        assert!(lb.fencing_token > la.fencing_token);
        assert!(matches!(
            m.validate(&la, 120),
            Err(LeaseError::Superseded { .. })
        ));
        m.validate(&lb, 120).unwrap();
    }

    #[test]
    fn stale_holder_cannot_release_new_lease() {
        let (dev, a, b, r) = ids();
        let mut m = LeaseManager::default();
        let la = m.acquire(&dev, &a, &r, 10, 0).unwrap();
        let lb = m.acquire(&dev, &b, &r, 10, 10).unwrap();
        assert!(m.release(&la).is_err());
        m.validate(&lb, 15).unwrap();
    }

    #[test]
    fn renew_requires_valid_lease() {
        let (dev, a, _, r) = ids();
        let mut m = LeaseManager::default();
        let la = m.acquire(&dev, &a, &r, 10, 0).unwrap();
        let renewed = m.renew(&la, 10, 5).unwrap();
        assert_eq!(renewed.expires_at, 15);
        assert!(matches!(
            m.renew(&la, 10, 20),
            Err(LeaseError::Expired { .. })
        ));
    }
}
