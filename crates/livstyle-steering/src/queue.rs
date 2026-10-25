use std::fmt;

use livstyle_domain::TaskId;
use serde::{Deserialize, Serialize};

use crate::{Action, Judgment};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InterventionId(pub String);

impl fmt::Display for InterventionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Every intervention ends in one of the user-visible results of AC-11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionStatus {
    Received,
    Classified,
    /// Waiting for the user to pick an intent.
    NeedsConfirmation,
    /// Held for the next send (runtime cannot steer, or input came back via `turn/steer_dropped`).
    Queued,
    /// Handed to the runtime as a steer; not yet confirmed consumed.
    Steered,
    Applied,
    /// Returned to the user unprocessed.
    Returned,
    Rejected,
}

impl InterventionStatus {
    pub fn is_settled(self) -> bool {
        matches!(
            self,
            InterventionStatus::Applied
                | InterventionStatus::Returned
                | InterventionStatus::Rejected
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterventionRecord {
    pub intervention_id: InterventionId,
    pub task_id: TaskId,
    pub received_at: u64,
    pub input: String,
    pub target_turn_id: Option<String>,
    pub judgment: Option<Judgment>,
    pub action: Option<Action>,
    pub status: InterventionStatus,
}

/// Intervention log for one Product Service instance. Inputs are recorded
/// before any processing so nothing is lost across restarts.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InterventionQueue {
    records: Vec<InterventionRecord>,
    seq: u64,
}

impl InterventionQueue {
    pub fn receive(
        &mut self,
        task_id: &TaskId,
        input: &str,
        target_turn_id: Option<&str>,
        now: u64,
    ) -> InterventionId {
        self.seq += 1;
        let id = InterventionId(format!("iv-{}", self.seq));
        self.records.push(InterventionRecord {
            intervention_id: id.clone(),
            task_id: task_id.clone(),
            received_at: now,
            input: input.to_owned(),
            target_turn_id: target_turn_id.map(str::to_owned),
            judgment: None,
            action: None,
            status: InterventionStatus::Received,
        });
        id
    }

    pub fn get(&self, id: &InterventionId) -> Option<&InterventionRecord> {
        self.records.iter().find(|r| &r.intervention_id == id)
    }

    fn get_mut(&mut self, id: &InterventionId) -> Option<&mut InterventionRecord> {
        self.records.iter_mut().find(|r| &r.intervention_id == id)
    }

    pub fn classify(&mut self, id: &InterventionId, judgment: Judgment) {
        if let Some(r) = self.get_mut(id) {
            r.judgment = Some(judgment);
            r.status = InterventionStatus::Classified;
        }
    }

    pub fn set_status(
        &mut self,
        id: &InterventionId,
        status: InterventionStatus,
        action: Option<Action>,
    ) {
        if let Some(r) = self.get_mut(id) {
            r.status = status;
            if action.is_some() {
                r.action = action;
            }
        }
    }

    /// Handles `turn/steer_dropped`: the runtime returns undrained steer texts
    /// in buffer order. Each matching steered record goes back to `Queued`
    /// (oldest first); texts with no matching record are recorded as new
    /// queued interventions so they are never silently lost.
    pub fn on_steer_dropped(
        &mut self,
        task_id: &TaskId,
        texts: &[String],
        now: u64,
    ) -> Vec<InterventionId> {
        let mut requeued = Vec::new();
        for text in texts {
            let found = self.records.iter_mut().find(|r| {
                &r.task_id == task_id && r.status == InterventionStatus::Steered && &r.input == text
            });
            let id = match found {
                Some(r) => {
                    r.status = InterventionStatus::Queued;
                    r.intervention_id.clone()
                }
                None => {
                    let id = self.receive(task_id, text, None, now);
                    self.set_status(&id, InterventionStatus::Queued, None);
                    id
                }
            };
            requeued.push(id);
        }
        requeued
    }

    /// Turn ended with no `turn/steer_dropped` naming these: the runtime consumed them.
    pub fn on_turn_completed(&mut self, task_id: &TaskId) {
        for r in &mut self.records {
            if &r.task_id == task_id && r.status == InterventionStatus::Steered {
                r.status = InterventionStatus::Applied;
            }
        }
    }

    pub fn queued(&self, task_id: &TaskId) -> Vec<&InterventionRecord> {
        self.records
            .iter()
            .filter(|r| &r.task_id == task_id && r.status == InterventionStatus::Queued)
            .collect()
    }

    pub fn unsettled(&self) -> Vec<&InterventionRecord> {
        self.records
            .iter()
            .filter(|r| !r.status.is_settled())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_steers_are_requeued_in_order() {
        let task = TaskId::new("t");
        let mut q = InterventionQueue::default();
        let a = q.receive(&task, "use 921600 baud", Some("turn-1"), 0);
        let b = q.receive(&task, "board has 8MB flash", Some("turn-1"), 1);
        q.set_status(&a, InterventionStatus::Steered, Some(Action::Steer));
        q.set_status(&b, InterventionStatus::Steered, Some(Action::Steer));

        let requeued = q.on_steer_dropped(&task, &["board has 8MB flash".into()], 2);
        assert_eq!(requeued, vec![b.clone()]);
        q.on_turn_completed(&task);

        assert_eq!(q.get(&a).unwrap().status, InterventionStatus::Applied);
        assert_eq!(q.get(&b).unwrap().status, InterventionStatus::Queued);
        assert_eq!(q.queued(&task).len(), 1);
    }

    #[test]
    fn unknown_dropped_text_is_kept() {
        let task = TaskId::new("t");
        let mut q = InterventionQueue::default();
        let ids = q.on_steer_dropped(&task, &["orphan".into()], 0);
        assert_eq!(q.get(&ids[0]).unwrap().status, InterventionStatus::Queued);
        assert_eq!(q.unsettled().len(), 1);
    }
}
