use std::collections::BTreeMap;

use livstyle_domain::{Interruptibility, RiskLevel, RiskPolicy, TaskId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Approval, ApprovalError, DeviceId, DispatchRequest, FencingToken, Lease, LeaseError,
    LeaseManager, Millis, OperationId, OperationIntent, OperationReceipt, Runner, RunnerId,
    RunnerOutcome, RunnerRejection,
};

/// Hardware operation state (PRD §7.2). `Dispatched` covers "handed to the
/// runner and possibly running"; the supervisor cannot observe finer progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationState {
    Prepared,
    Authorized,
    Dispatched,
    Completed,
    Rejected,
    Failed,
    Cancelled,
    OutcomeUnknown,
    Reconciled,
}

impl OperationState {
    pub fn is_final(self) -> bool {
        matches!(
            self,
            OperationState::Completed
                | OperationState::Rejected
                | OperationState::Failed
                | OperationState::Cancelled
                | OperationState::Reconciled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciliation {
    /// Whether device inspection shows the operation took effect.
    pub applied: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: OperationId,
    pub intent: OperationIntent,
    pub parameters_digest: String,
    pub state: OperationState,
    pub approval: Option<Approval>,
    pub fencing_token: Option<FencingToken>,
    pub cancel_requested: bool,
    pub receipt: Option<OperationReceipt>,
    pub rejection: Option<RunnerRejection>,
    pub reconciliation: Option<Reconciliation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub at: Millis,
    pub operation_id: Option<OperationId>,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// Not yet dispatched; the operation will never run.
    Cancelled,
    /// Interruptible operation; the caller should signal the runner to stop.
    InterruptRequested,
    /// Non-interruptible operation; it runs to completion and the task stops afterwards.
    DeferredUntilComplete,
    AlreadyFinished(OperationState),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SupervisorError {
    #[error("unknown operation {0}")]
    UnknownOperation(OperationId),
    #[error("{risk:?} operations are denied by policy")]
    PolicyDenied { risk: RiskLevel },
    #[error(transparent)]
    Approval(#[from] ApprovalError),
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error("operation in state {state:?} cannot be authorized")]
    NotAuthorizable { state: OperationState },
    #[error("operation in state {state:?} cannot be dispatched")]
    NotDispatchable { state: OperationState },
    #[error("lease does not cover this task, device and runner")]
    LeaseMismatch,
    #[error("forcing an abort requires explicit user confirmation")]
    ConfirmationRequired,
    #[error("operation in state {state:?} cannot accept this transition")]
    InvalidState { state: OperationState },
}

/// Deterministic hardware safety boundary. Agents and judges call into it;
/// nothing they produce can override its rejections.
#[derive(Debug, Default)]
pub struct Supervisor {
    leases: LeaseManager,
    ops: BTreeMap<OperationId, OperationRecord>,
    audit: Vec<AuditEvent>,
    seq: u64,
}

impl Supervisor {
    pub fn acquire_lease(
        &mut self,
        device: &DeviceId,
        task: &TaskId,
        runner: &RunnerId,
        ttl: Millis,
        now: Millis,
    ) -> Result<Lease, LeaseError> {
        let result = self.leases.acquire(device, task, runner, ttl, now);
        let detail = match &result {
            Ok(l) => format!("granted {} token {}", l.lease_id, l.fencing_token.0),
            Err(e) => format!("refused: {e}"),
        };
        self.log(
            now,
            None,
            "lease.acquire",
            format!("{device} for {task}: {detail}"),
        );
        result
    }

    pub fn release_lease(&mut self, lease: &Lease, now: Millis) -> Result<(), LeaseError> {
        let result = self.leases.release(lease);
        self.log(
            now,
            None,
            "lease.release",
            format!("{} -> {result:?}", lease.lease_id),
        );
        result
    }

    pub fn record(&self, op: &OperationId) -> Option<&OperationRecord> {
        self.ops.get(op)
    }

    pub fn audit(&self) -> &[AuditEvent] {
        &self.audit
    }

    /// Persists the intent before anything runs. R0/R1 are authorized
    /// immediately, R2 waits for an approval, R3/R4 are rejected.
    pub fn prepare(
        &mut self,
        intent: OperationIntent,
        now: Millis,
    ) -> Result<OperationId, SupervisorError> {
        self.seq += 1;
        let operation_id = OperationId::new(format!("op-{}", self.seq));
        let risk = intent.risk();
        let state = match risk.mvp_policy() {
            RiskPolicy::Allow => OperationState::Authorized,
            RiskPolicy::RequireApproval => OperationState::Prepared,
            RiskPolicy::Deny => OperationState::Rejected,
        };
        let record = OperationRecord {
            operation_id: operation_id.clone(),
            parameters_digest: intent.params.digest(),
            intent,
            state,
            approval: None,
            fencing_token: None,
            cancel_requested: false,
            receipt: None,
            rejection: None,
            reconciliation: None,
        };
        self.log(
            now,
            Some(&operation_id),
            "op.prepare",
            format!(
                "{:?} {risk:?} -> {state:?}",
                record.intent.params.operation_type
            ),
        );
        self.ops.insert(operation_id.clone(), record);
        if state == OperationState::Rejected {
            return Err(SupervisorError::PolicyDenied { risk });
        }
        Ok(operation_id)
    }

    pub fn authorize(
        &mut self,
        op: &OperationId,
        approval: Approval,
        now: Millis,
    ) -> Result<(), SupervisorError> {
        let record = self.get_mut(op)?;
        if record.state != OperationState::Prepared {
            return Err(SupervisorError::NotAuthorizable {
                state: record.state,
            });
        }
        let check = approval.check(
            record.intent.params.operation_type,
            &record.parameters_digest,
            now,
        );
        if let Err(e) = check {
            self.log(now, Some(op), "op.authorize.denied", e.to_string());
            return Err(e.into());
        }
        record.approval = Some(approval);
        record.state = OperationState::Authorized;
        self.log(now, Some(op), "op.authorize", "approved".into());
        Ok(())
    }

    /// Validates lease and approval and hands back the request for the runner.
    pub fn begin_dispatch(
        &mut self,
        op: &OperationId,
        lease: &Lease,
        now: Millis,
    ) -> Result<DispatchRequest, SupervisorError> {
        let record = self
            .ops
            .get(op)
            .ok_or_else(|| SupervisorError::UnknownOperation(op.clone()))?;
        if record.state != OperationState::Authorized {
            return Err(SupervisorError::NotDispatchable {
                state: record.state,
            });
        }
        let params = &record.intent.params;
        if lease.task_id != record.intent.task_id
            || lease.device_id != params.device_id
            || lease.runner_id != params.runner_id
        {
            self.log(now, Some(op), "op.dispatch.denied", "lease mismatch".into());
            return Err(SupervisorError::LeaseMismatch);
        }
        if let Err(e) = self.leases.validate(lease, now) {
            self.log(now, Some(op), "op.dispatch.denied", e.to_string());
            return Err(e.into());
        }
        if let Some(approval) = &record.approval {
            if let Err(e) = approval.check(params.operation_type, &record.parameters_digest, now) {
                self.log(now, Some(op), "op.dispatch.denied", e.to_string());
                return Err(e.into());
            }
        }

        let request = DispatchRequest {
            operation_id: op.clone(),
            task_id: record.intent.task_id.clone(),
            fencing_token: lease.fencing_token,
            parameters_digest: record.parameters_digest.clone(),
            params: params.clone(),
        };
        let record = self.get_mut(op)?;
        record.state = OperationState::Dispatched;
        record.fencing_token = Some(lease.fencing_token);
        self.log(
            now,
            Some(op),
            "op.dispatch",
            format!("token {}", lease.fencing_token.0),
        );
        Ok(request)
    }

    /// Applies the runner's outcome. A late receipt for an operation already
    /// marked `OutcomeUnknown` reconciles it instead of resurrecting it.
    pub fn complete(
        &mut self,
        op: &OperationId,
        outcome: RunnerOutcome,
        now: Millis,
    ) -> Result<OperationState, SupervisorError> {
        let record = self.get_mut(op)?;
        let next = match (record.state, outcome) {
            (OperationState::Dispatched, RunnerOutcome::Completed(r)) => {
                record.receipt = Some(r);
                OperationState::Completed
            }
            (OperationState::Dispatched, RunnerOutcome::Failed(r)) => {
                record.receipt = Some(r);
                OperationState::Failed
            }
            (OperationState::Dispatched, RunnerOutcome::Rejected(rej)) => {
                record.rejection = Some(rej);
                OperationState::Rejected
            }
            (OperationState::Dispatched, RunnerOutcome::OutcomeUnknown { .. }) => {
                OperationState::OutcomeUnknown
            }
            (OperationState::OutcomeUnknown, RunnerOutcome::Completed(r)) => {
                record.receipt = Some(r);
                record.reconciliation = Some(Reconciliation {
                    applied: true,
                    evidence: "late runner receipt".into(),
                });
                OperationState::Reconciled
            }
            (OperationState::OutcomeUnknown, RunnerOutcome::Failed(r)) => {
                record.receipt = Some(r);
                OperationState::OutcomeUnknown
            }
            (OperationState::OutcomeUnknown, RunnerOutcome::Rejected(rej)) => {
                record.rejection = Some(rej);
                record.reconciliation = Some(Reconciliation {
                    applied: false,
                    evidence: "runner rejected before touching the device".into(),
                });
                OperationState::Reconciled
            }
            (OperationState::OutcomeUnknown, RunnerOutcome::OutcomeUnknown { .. }) => {
                OperationState::OutcomeUnknown
            }
            (state, _) => return Err(SupervisorError::InvalidState { state }),
        };
        record.state = next;
        self.log(now, Some(op), "op.complete", format!("{next:?}"));
        Ok(next)
    }

    /// Convenience for synchronous runners.
    pub fn dispatch(
        &mut self,
        op: &OperationId,
        lease: &Lease,
        runner: &mut dyn Runner,
        now: Millis,
    ) -> Result<OperationState, SupervisorError> {
        let request = self.begin_dispatch(op, lease, now)?;
        let outcome = runner.execute(&request);
        self.complete(op, outcome, now)
    }

    pub fn request_cancel(
        &mut self,
        op: &OperationId,
        now: Millis,
    ) -> Result<CancelOutcome, SupervisorError> {
        let record = self.get_mut(op)?;
        let outcome = match record.state {
            OperationState::Prepared | OperationState::Authorized => {
                record.state = OperationState::Cancelled;
                CancelOutcome::Cancelled
            }
            OperationState::Dispatched => {
                record.cancel_requested = true;
                match record.intent.params.operation_type.interruptibility() {
                    Interruptibility::Free => CancelOutcome::InterruptRequested,
                    Interruptibility::SafePoint | Interruptibility::NonInterruptible => {
                        CancelOutcome::DeferredUntilComplete
                    }
                }
            }
            state => CancelOutcome::AlreadyFinished(state),
        };
        self.log(now, Some(op), "op.cancel", format!("{outcome:?}"));
        Ok(outcome)
    }

    /// User-confirmed abort of a dispatched operation. The result can no
    /// longer be trusted, so it becomes `OutcomeUnknown` (PRD §8.5).
    pub fn force_abort(
        &mut self,
        op: &OperationId,
        user_confirmed: bool,
        now: Millis,
    ) -> Result<(), SupervisorError> {
        if !user_confirmed {
            return Err(SupervisorError::ConfirmationRequired);
        }
        let record = self.get_mut(op)?;
        if record.state != OperationState::Dispatched {
            return Err(SupervisorError::InvalidState {
                state: record.state,
            });
        }
        record.state = OperationState::OutcomeUnknown;
        self.log(now, Some(op), "op.force_abort", "user confirmed".into());
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        op: &OperationId,
        finding: Reconciliation,
        now: Millis,
    ) -> Result<(), SupervisorError> {
        let record = self.get_mut(op)?;
        if record.state != OperationState::OutcomeUnknown {
            return Err(SupervisorError::InvalidState {
                state: record.state,
            });
        }
        let detail = format!("applied={} ({})", finding.applied, finding.evidence);
        record.reconciliation = Some(finding);
        record.state = OperationState::Reconciled;
        self.log(now, Some(op), "op.reconcile", detail);
        Ok(())
    }

    fn get_mut(&mut self, op: &OperationId) -> Result<&mut OperationRecord, SupervisorError> {
        self.ops
            .get_mut(op)
            .ok_or_else(|| SupervisorError::UnknownOperation(op.clone()))
    }

    fn log(&mut self, at: Millis, op: Option<&OperationId>, kind: &str, detail: String) {
        self.audit.push(AuditEvent {
            at,
            operation_id: op.cloned(),
            kind: kind.to_owned(),
            detail,
        });
    }
}
