use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TaskId;

/// Product-level task status (PRD §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Planning,
    Running,
    AwaitingApproval,
    Replanning,
    Paused,
    Verifying,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
    OutcomeUnknown,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }

    pub fn can_transition_to(self, next: TaskStatus) -> bool {
        use TaskStatus::*;
        match self {
            Queued => matches!(next, Planning | Cancelled),
            Planning => matches!(next, Running | Blocked | Failed | Cancelled),
            Running => matches!(
                next,
                AwaitingApproval
                    | Replanning
                    | Paused
                    | Verifying
                    | Failed
                    | Cancelled
                    | Blocked
                    | OutcomeUnknown
            ),
            AwaitingApproval => matches!(next, Running | Replanning | Cancelled | Blocked),
            Replanning => matches!(next, Running | Failed | Cancelled | Blocked),
            Paused => matches!(next, Running | Replanning | Cancelled),
            Verifying => matches!(
                next,
                Succeeded | Failed | Running | Replanning | Cancelled | OutcomeUnknown
            ),
            // Leaving OutcomeUnknown requires reconciliation by the caller.
            OutcomeUnknown => matches!(next, Running | Failed | Blocked | Cancelled),
            Blocked => matches!(next, Running | Planning | Failed | Cancelled),
            Succeeded | Failed | Cancelled => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskError {
    #[error("version conflict: expected {expected}, current {current}")]
    VersionConflict { expected: u64, current: u64 },
    #[error("illegal transition {from:?} -> {to:?}")]
    IllegalTransition { from: TaskStatus, to: TaskStatus },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductTask {
    pub task_id: TaskId,
    pub workspace_id: String,
    pub runtime_id: String,
    pub runtime_session_id: Option<String>,
    pub status: TaskStatus,
    pub requirement_version: u64,
    pub plan_version: u64,
    pub current_node_id: Option<String>,
    /// Optimistic concurrency version; every mutation bumps it.
    pub version: u64,
    pub parent_task_id: Option<TaskId>,
    /// Conversation lines for this task. Older rows omit the field.
    #[serde(default)]
    pub transcript: Vec<String>,
}

impl ProductTask {
    pub fn new(
        task_id: TaskId,
        workspace_id: impl Into<String>,
        runtime_id: impl Into<String>,
    ) -> Self {
        Self {
            task_id,
            workspace_id: workspace_id.into(),
            runtime_id: runtime_id.into(),
            runtime_session_id: None,
            status: TaskStatus::Queued,
            requirement_version: 1,
            plan_version: 0,
            current_node_id: None,
            version: 0,
            parent_task_id: None,
            transcript: Vec::new(),
        }
    }

    fn check_version(&self, expected: u64) -> Result<(), TaskError> {
        if self.version != expected {
            return Err(TaskError::VersionConflict {
                expected,
                current: self.version,
            });
        }
        Ok(())
    }

    pub fn transition(&mut self, expected_version: u64, next: TaskStatus) -> Result<(), TaskError> {
        self.check_version(expected_version)?;
        if !self.status.can_transition_to(next) {
            return Err(TaskError::IllegalTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        self.version += 1;
        Ok(())
    }

    /// Records a requirement amendment; downstream plan invalidation is the caller's job.
    pub fn amend_requirement(&mut self, expected_version: u64) -> Result<u64, TaskError> {
        self.check_version(expected_version)?;
        self.requirement_version += 1;
        self.version += 1;
        Ok(self.requirement_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> ProductTask {
        ProductTask::new(TaskId::new("t1"), "ws", "octos-native")
    }

    #[test]
    fn stale_writer_is_rejected() {
        let mut t = task();
        t.transition(0, TaskStatus::Planning).unwrap();
        let err = t.transition(0, TaskStatus::Running).unwrap_err();
        assert_eq!(
            err,
            TaskError::VersionConflict {
                expected: 0,
                current: 1
            }
        );
    }

    #[test]
    fn terminal_states_are_final() {
        let mut t = task();
        t.transition(0, TaskStatus::Cancelled).unwrap();
        assert!(t.transition(1, TaskStatus::Planning).is_err());
    }

    #[test]
    fn running_can_replan_and_resume() {
        let mut t = task();
        for (v, s) in [
            TaskStatus::Planning,
            TaskStatus::Running,
            TaskStatus::Replanning,
            TaskStatus::Running,
        ]
        .into_iter()
        .enumerate()
        {
            t.transition(v as u64, s).unwrap();
        }
        assert_eq!(t.amend_requirement(4).unwrap(), 2);
    }

    #[test]
    fn queued_cannot_jump_to_succeeded() {
        assert!(!TaskStatus::Queued.can_transition_to(TaskStatus::Succeeded));
        assert!(!TaskStatus::Running.can_transition_to(TaskStatus::Succeeded));
    }
}
