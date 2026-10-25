//! Product service (ADR-002). Owns the task, the plan revision and every
//! mid-run input. Runtimes are attached by id; this crate does not open
//! sockets, so the same path runs in tests and behind the CLI.

use livstyle_domain::{ProductTask, TaskId, TaskStatus};
use livstyle_steering::{
    InterventionContext, Judge, Resolution, ResolvePolicy, RuntimeCaps, Stage, resolve,
};
use livstyle_store::{OperationState, RuntimeMapping, Store, StoreError};
use serde_json::{Value, json};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Task(#[from] livstyle_domain::TaskError),
    #[error("judge: {0}")]
    Judge(String),
}

pub struct Service {
    store: Store,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterventionOutcome {
    pub id: String,
    pub resolution: Resolution,
    pub task: ProductTask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// Operations that were mid-write when the process stopped.
    pub outcome_unknown: Vec<String>,
    /// Inputs that were stored and never settled.
    pub pending: Vec<(String, String, String)>,
}

impl Service {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, CoreError> {
        Ok(Self {
            store: Store::open(path)?,
        })
    }

    pub fn open_in_memory() -> Result<Self, CoreError> {
        Ok(Self {
            store: Store::open_in_memory()?,
        })
    }

    pub fn is_empty(&self) -> Result<bool, CoreError> {
        Ok(self.store.task_ids()?.is_empty())
    }

    pub fn task_ids(&self) -> Result<Vec<String>, CoreError> {
        Ok(self.store.task_ids()?)
    }

    pub fn delete_task(&mut self, id: &str) -> Result<(), CoreError> {
        self.store.delete_task(id)?;
        Ok(())
    }

    pub fn create_task(
        &mut self,
        id: &str,
        workspace: &str,
        runtime: &str,
    ) -> Result<ProductTask, CoreError> {
        let task = ProductTask::new(TaskId::new(id), workspace, runtime);
        self.store.create_task(id, &status_of(&task), &task)?;
        Ok(self.task(id)?.1)
    }

    pub fn task(&self, id: &str) -> Result<(u64, ProductTask), CoreError> {
        let row = self.store.task::<ProductTask>(id)?;
        Ok((row.version, row.value))
    }

    /// Remembers which Octos session or Cursor agent this task is bound to.
    pub fn bind_runtime(
        &mut self,
        task_id: &str,
        runtime: &str,
        kind: &str,
        external_id: &str,
    ) -> Result<(), CoreError> {
        let (version, mut task) = self.task(task_id)?;
        task.runtime_session_id = Some(external_id.to_owned());
        self.store.update_task(
            task_id,
            version,
            &status_of(&task),
            &task,
            Some((
                "runtime.bound",
                &json!({ "runtime": runtime, "id": external_id }),
            )),
        )?;
        self.store.upsert_runtime_mapping(&RuntimeMapping {
            task_id: task_id.into(),
            runtime: runtime.into(),
            kind: kind.into(),
            external_id: external_id.into(),
            resume_offset: None,
        })?;
        Ok(())
    }

    /// Extra runtime object (a Cursor run id) that must not replace the
    /// session or agent id stored on the task.
    pub fn record_mapping(
        &mut self,
        task_id: &str,
        runtime: &str,
        kind: &str,
        external_id: &str,
    ) -> Result<(), CoreError> {
        self.store.upsert_runtime_mapping(&RuntimeMapping {
            task_id: task_id.into(),
            runtime: runtime.into(),
            kind: kind.into(),
            external_id: external_id.into(),
            resume_offset: None,
        })?;
        Ok(())
    }

    pub fn set_transcript(&mut self, id: &str, lines: &[String]) -> Result<(), CoreError> {
        let (version, mut task) = self.task(id)?;
        task.transcript = lines.iter().rev().take(2000).cloned().rev().collect();
        self.store.update_task(
            id,
            version,
            &status_of(&task),
            &task,
            Some(("transcript.saved", &json!({ "lines": task.transcript.len() }))),
        )?;
        Ok(())
    }

    pub fn plan_summary(&self, id: &str) -> Result<String, CoreError> {
        let Some(saved) = self.store.latest_plan::<livstyle_domain::Plan>(id)? else {
            return Ok(String::new());
        };
        let mut lines = Vec::new();
        for node in saved.value.nodes() {
            lines.push(format!("{:?}  {}  {:?}", node.status, node.title, node.kind));
        }
        Ok(lines.join("\n"))
    }

    pub fn mapping(
        &self,
        task_id: &str,
        runtime: &str,
        kind: &str,
    ) -> Result<Option<String>, CoreError> {
        Ok(self
            .store
            .runtime_mappings(task_id)?
            .into_iter()
            .find(|row| row.runtime == runtime && row.kind == kind)
            .map(|row| row.external_id))
    }

    pub fn transition(&mut self, id: &str, next: TaskStatus) -> Result<ProductTask, CoreError> {
        let (version, mut task) = self.task(id)?;
        task.transition(task.version, next)?;
        self.store.update_task(
            id,
            version,
            &status_of(&task),
            &task,
            Some(("task.transition", &json!({ "status": status_of(&task) }))),
        )?;
        Ok(task)
    }

    /// Persists the input, classifies it, and applies the resolution to the task.
    /// Hardware approval is never granted here.
    pub fn intervene(
        &mut self,
        task_id: &str,
        intervention_id: &str,
        input: &str,
        judge: &dyn Judge,
        caps: RuntimeCaps,
        stage: Stage,
    ) -> Result<InterventionOutcome, CoreError> {
        self.store
            .record_intervention(intervention_id, task_id, input)?;
        let (_, task) = self.task(task_id)?;
        let judgment = judge
            .judge(&InterventionContext {
                task_summary: task.workspace_id.clone(),
                current_node: task.current_node_id.clone(),
                plan_outline: Vec::new(),
                input: input.into(),
            })
            .map_err(|e| CoreError::Judge(e.to_string()))?;
        let resolution = resolve(&judgment, caps, ResolvePolicy::default(), stage);
        let status = match &resolution {
            Resolution::Apply(_) => "applied",
            Resolution::EscalateToSystemTwo => "escalated",
            Resolution::AskUser { .. } => "needs_confirmation",
        };
        self.store.update_intervention(
            intervention_id,
            status,
            Some(&judgment),
            Some(&resolution),
        )?;
        let task = self.apply(task_id, &resolution)?;
        Ok(InterventionOutcome {
            id: intervention_id.into(),
            resolution,
            task,
        })
    }

    fn apply(&mut self, task_id: &str, resolution: &Resolution) -> Result<ProductTask, CoreError> {
        let Resolution::Apply(action) = resolution else {
            return Ok(self.task(task_id)?.1);
        };
        use livstyle_steering::Action::*;
        let (store_version, mut task) = self.task(task_id)?;
        let changed = match action {
            CancelTask => move_to(&mut task, TaskStatus::Cancelled)?,
            PauseAtBoundary => move_to(&mut task, TaskStatus::Paused)?,
            InterruptAndReplan { .. } | ReplanAfterTurn { .. } => {
                if task.status.can_transition_to(TaskStatus::Replanning) {
                    let version = task.version;
                    task.amend_requirement(version)?;
                    task.transition(task.version, TaskStatus::Replanning)?;
                    true
                } else {
                    false
                }
            }
            Steer | QueueForNextSend | InterruptStep | Reprioritize | SideAnswer | NewTask => false,
        };
        if changed {
            self.store.update_task(
                task_id,
                store_version,
                &status_of(&task),
                &task,
                Some((
                    "intervention.applied",
                    &json!({ "status": status_of(&task) }),
                )),
            )?;
        }
        Ok(self.task(task_id)?.1)
    }

    /// Startup: unsettled inputs come back, and a write that was in flight
    /// becomes OutcomeUnknown instead of being sent again.
    pub fn recover(&mut self) -> Result<Recovery, CoreError> {
        let unknown = self.store.recover_inflight_operations()?;
        let mut pending = Vec::new();
        for id in self.store.tasks_with_status(&[
            "Queued",
            "Planning",
            "Running",
            "AwaitingApproval",
            "Replanning",
            "Paused",
            "Verifying",
            "Blocked",
            "OutcomeUnknown",
        ])? {
            for (iid, _seq, input, status) in self.store.pending_interventions(&id)? {
                pending.push((id.clone(), iid, format!("{status}: {input}")));
            }
        }
        Ok(Recovery {
            outcome_unknown: unknown.into_iter().map(|op| op.id).collect(),
            pending,
        })
    }

    pub fn interventions(&self, task_id: &str) -> Result<Vec<(String, String, String)>, CoreError> {
        Ok(self
            .store
            .interventions(task_id)?
            .into_iter()
            .map(|(id, _seq, input, status)| (id, input, status))
            .collect())
    }

    /// User answered a confirmation card. Does not grant a hardware approval.
    pub fn settle(&mut self, intervention_id: &str, status: &str) -> Result<(), CoreError> {
        self.store
            .update_intervention::<Value, Value>(intervention_id, status, None, None)?;
        Ok(())
    }

    /// Records a requirement change and moves the task to replanning when legal.
    pub fn force_replan(&mut self, task_id: &str) -> Result<ProductTask, CoreError> {
        let (store_version, mut task) = self.task(task_id)?;
        if task.status.can_transition_to(TaskStatus::Replanning) {
            let version = task.version;
            task.amend_requirement(version)?;
            task.transition(task.version, TaskStatus::Replanning)?;
            self.store.update_task(
                task_id,
                store_version,
                &status_of(&task),
                &task,
                Some(("intervention.confirmed", &json!({ "status": "Replanning" }))),
            )?;
        }
        Ok(self.task(task_id)?.1)
    }

    pub fn save_plan<T: serde::Serialize>(
        &mut self,
        task_id: &str,
        plan: &T,
        reason: &str,
    ) -> Result<u64, CoreError> {
        Ok(self.store.save_plan(task_id, plan, reason)?)
    }

    /// Records an approval without dispatching the write.
    pub fn authorize(
        &mut self,
        task_id: &str,
        op_id: &str,
        digest: &str,
        device: &str,
    ) -> Result<(), CoreError> {
        self.store
            .record_operation(op_id, task_id, "flash", digest, device)?;
        self.store
            .transition_operation(op_id, OperationState::Authorized, None, &Value::Null)?;
        Ok(())
    }

    pub fn note_dispatched(
        &mut self,
        task_id: &str,
        op_id: &str,
        digest: &str,
        device: &str,
    ) -> Result<(), CoreError> {
        self.store
            .record_operation(op_id, task_id, "flash", digest, device)?;
        self.store
            .transition_operation(op_id, OperationState::Authorized, None, &Value::Null)?;
        self.store.transition_operation(
            op_id,
            OperationState::Dispatched,
            Some(1),
            &Value::Null,
        )?;
        Ok(())
    }
}

fn move_to(task: &mut ProductTask, next: TaskStatus) -> Result<bool, CoreError> {
    if task.status.can_transition_to(next) {
        task.transition(task.version, next)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn status_of(task: &ProductTask) -> String {
    format!("{:?}", task.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use livstyle_steering::{Action, RuleJudge};

    fn caps() -> RuntimeCaps {
        RuntimeCaps {
            steer: true,
            interrupt: true,
        }
    }

    #[test]
    fn cancel_is_persisted_and_applied() {
        let mut svc = Service::open_in_memory().unwrap();
        svc.create_task("t1", "blink on c5", "octos").unwrap();
        svc.transition("t1", TaskStatus::Planning).unwrap();
        svc.transition("t1", TaskStatus::Running).unwrap();
        svc.bind_runtime("t1", "octos", "session", "livstyle:s1")
            .unwrap();
        let out = svc
            .intervene("t1", "i1", "不做了", &RuleJudge, caps(), Stage::First)
            .unwrap();
        assert!(matches!(
            out.resolution,
            Resolution::Apply(Action::CancelTask)
        ));
        assert_eq!(out.task.status, TaskStatus::Cancelled);
        assert_eq!(out.task.runtime_session_id.as_deref(), Some("livstyle:s1"));
        assert!(svc.recover().unwrap().pending.is_empty());
    }

    #[test]
    fn run_mapping_does_not_replace_session_id() {
        let mut svc = Service::open_in_memory().unwrap();
        svc.create_task("t1", "blink", "cursor").unwrap();
        svc.bind_runtime("t1", "cursor", "agent", "agent-1")
            .unwrap();
        svc.record_mapping("t1", "cursor", "run", "run-9")
            .unwrap();
        assert_eq!(
            svc.task("t1").unwrap().1.runtime_session_id.as_deref(),
            Some("agent-1")
        );
        assert_eq!(
            svc.mapping("t1", "cursor", "run").unwrap().as_deref(),
            Some("run-9")
        );
    }

    #[test]
    fn transcript_stays_on_its_own_task() {
        let mut svc = Service::open_in_memory().unwrap();
        svc.create_task("a", "板子 A", "octos").unwrap();
        svc.create_task("b", "板子 B", "octos").unwrap();
        svc.set_transcript("a", &["只属于 A".into()]).unwrap();
        svc.set_transcript("b", &["只属于 B".into()]).unwrap();
        assert_eq!(svc.task("a").unwrap().1.transcript, vec!["只属于 A"]);
        assert_eq!(svc.task("b").unwrap().1.transcript, vec!["只属于 B"]);
        assert!(svc.plan_summary("a").unwrap().is_empty());
    }

    #[test]
    fn vague_input_is_kept_for_system_two() {
        let mut svc = Service::open_in_memory().unwrap();
        svc.create_task("t1", "blink", "octos").unwrap();
        svc.transition("t1", TaskStatus::Planning).unwrap();
        svc.transition("t1", TaskStatus::Running).unwrap();
        let out = svc
            .intervene("t1", "i1", "嗯", &RuleJudge, caps(), Stage::First)
            .unwrap();
        assert!(matches!(out.resolution, Resolution::EscalateToSystemTwo));
        assert_eq!(out.task.status, TaskStatus::Running);
        let recovered = svc.recover().unwrap();
        assert_eq!(recovered.pending.len(), 1);
        assert!(recovered.pending[0].2.contains("嗯"));
    }

    #[test]
    fn restart_does_not_redispatch_a_flash() {
        let dir = std::env::temp_dir().join(format!("livstyle-core-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");
        {
            let mut svc = Service::open(&path).unwrap();
            svc.create_task("t1", "blink", "octos").unwrap();
            svc.note_dispatched("t1", "op1", "abc", "c5").unwrap();
        }
        let mut svc = Service::open(&path).unwrap();
        let recovered = svc.recover().unwrap();
        assert_eq!(recovered.outcome_unknown, ["op1"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
