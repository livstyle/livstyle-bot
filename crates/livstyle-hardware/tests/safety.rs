//! Spike 3: supervisor + mock runner fault injection, mapped to PRD acceptance criteria.

use livstyle_domain::{RiskLevel, TaskId};
use livstyle_hardware::mock::{Fault, MockRunner};
use livstyle_hardware::{
    Approval, ApprovalError, ApprovalId, CancelOutcome, DeviceId, DispatchRequest, FencingToken,
    LeaseError, OperationId, OperationIntent, OperationParams, OperationState, OperationType,
    Reconciliation, Runner, RunnerId, RunnerOutcome, RunnerRejection, Supervisor, SupervisorError,
    TargetChip,
};
use livstyle_profiles::{ChipRevision, RevisionPolicy};

const RUNNER: &str = "runner-local";
const HOUR: u64 = 3_600_000;

fn device() -> DeviceId {
    DeviceId::new("esp-1")
}

fn target(chip: &str) -> TargetChip {
    TargetChip {
        chip: chip.to_owned(),
        revisions: RevisionPolicy::default(),
        chip_profile_version: format!("{chip}@1"),
    }
}

fn flash_params(chip: &str, artifact: &str) -> OperationParams {
    OperationParams {
        operation_type: OperationType::Flash,
        device_id: device(),
        runner_id: RunnerId::new(RUNNER),
        target: Some(target(chip)),
        board_profile_id: Some("esp32s3-devkitc-1-n8r8".into()),
        artifact_sha256: Some(artifact.to_owned()),
        regions: vec![],
        extra: Default::default(),
    }
}

fn intent(task: &str, params: OperationParams) -> OperationIntent {
    OperationIntent {
        task_id: TaskId::new(task),
        params,
    }
}

fn approve(sup: &Supervisor, op: &OperationId, now: u64) -> Approval {
    let record = sup.record(op).unwrap();
    Approval {
        approval_id: ApprovalId::new(format!("ap-{op}")),
        operation_type: record.intent.params.operation_type,
        risk: record.intent.risk(),
        parameters_digest: record.parameters_digest.clone(),
        approved_by: "user".into(),
        granted_at: now,
        expires_at: now + HOUR,
    }
}

fn setup(chip: &str) -> (Supervisor, MockRunner) {
    let mut runner = MockRunner::new(RUNNER);
    runner.add_device(&device(), chip, ChipRevision::new(0, 2));
    (Supervisor::default(), runner)
}

/// Prepares and approves a flash for `task`.
fn approved_flash(sup: &mut Supervisor, task: &str, chip: &str, artifact: &str) -> OperationId {
    let op = sup
        .prepare(intent(task, flash_params(chip, artifact)), 0)
        .unwrap();
    let approval = approve(sup, &op, 0);
    sup.authorize(&op, approval, 0).unwrap();
    op
}

#[test]
fn happy_path_flash_writes_once() {
    let (mut sup, mut runner) = setup("esp32s3");
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();

    let state = sup.dispatch(&op, &lease, &mut runner, 1).unwrap();

    assert_eq!(state, OperationState::Completed);
    let dev = runner.device(&device()).unwrap();
    assert_eq!(dev.flash_writes, 1);
    assert_eq!(dev.image_sha256.as_deref(), Some("sha-a"));
    assert_eq!(
        sup.record(&op)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .fencing_token,
        lease.fencing_token
    );
}

#[test]
fn r2_requires_approval_before_dispatch() {
    let (mut sup, _) = setup("esp32s3");
    let op = sup
        .prepare(intent("t1", flash_params("esp32s3", "sha-a")), 0)
        .unwrap();
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();
    assert_eq!(
        sup.begin_dispatch(&op, &lease, 1).unwrap_err(),
        SupervisorError::NotDispatchable {
            state: OperationState::Prepared
        }
    );
}

#[test]
fn r3_and_r4_are_denied_at_prepare() {
    let (mut sup, _) = setup("esp32s3");
    for (ty, risk) in [
        (OperationType::EraseFlash, RiskLevel::R3),
        (OperationType::WriteEfuse, RiskLevel::R4),
    ] {
        let mut params = flash_params("esp32s3", "sha-a");
        params.operation_type = ty;
        assert_eq!(
            sup.prepare(intent("t1", params), 0).unwrap_err(),
            SupervisorError::PolicyDenied { risk }
        );
    }
}

/// AC-03: changing artifact hash, device, operation type or target chip voids the approval.
#[test]
fn ac03_approval_binds_exact_parameters() {
    let (mut sup, _) = setup("esp32s3");
    let original = sup
        .prepare(intent("t1", flash_params("esp32s3", "sha-a")), 0)
        .unwrap();
    let approval = approve(&sup, &original, 0);

    let mut variants = vec![
        flash_params("esp32s3", "sha-b"),
        flash_params("esp32c3", "sha-a"),
    ];
    let mut other_device = flash_params("esp32s3", "sha-a");
    other_device.device_id = DeviceId::new("esp-2");
    variants.push(other_device);

    for params in variants {
        let op = sup.prepare(intent("t1", params), 0).unwrap();
        assert_eq!(
            sup.authorize(&op, approval.clone(), 0).unwrap_err(),
            SupervisorError::Approval(ApprovalError::DigestMismatch)
        );
    }

    let mut reset = flash_params("esp32s3", "sha-a");
    reset.operation_type = OperationType::Reset;
    let op = sup.prepare(intent("t1", reset), 0).unwrap();
    assert!(matches!(
        sup.authorize(&op, approval.clone(), 0).unwrap_err(),
        SupervisorError::Approval(ApprovalError::OperationMismatch { .. })
    ));

    assert!(matches!(
        sup.authorize(&original, approval, HOUR + 1).unwrap_err(),
        SupervisorError::Approval(ApprovalError::Expired { .. })
    ));
}

/// AC-04: disconnect mid-flash -> OutcomeUnknown, never re-dispatched, reconciled from the device.
#[test]
fn ac04_disconnect_is_outcome_unknown_and_not_retried() {
    let (mut sup, mut runner) = setup("esp32s3");
    runner.inject(Fault::DisconnectAfterWrite);
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();

    assert_eq!(
        sup.dispatch(&op, &lease, &mut runner, 1).unwrap(),
        OperationState::OutcomeUnknown
    );
    assert_eq!(
        sup.dispatch(&op, &lease, &mut runner, 2).unwrap_err(),
        SupervisorError::NotDispatchable {
            state: OperationState::OutcomeUnknown
        }
    );
    assert_eq!(runner.device(&device()).unwrap().flash_writes, 1);

    let image = runner.device(&device()).unwrap().image_sha256.clone();
    sup.reconcile(
        &op,
        Reconciliation {
            applied: image.as_deref() == Some("sha-a"),
            evidence: "read back image hash".into(),
        },
        3,
    )
    .unwrap();
    let record = sup.record(&op).unwrap();
    assert_eq!(record.state, OperationState::Reconciled);
    assert!(record.reconciliation.as_ref().unwrap().applied);
}

/// AC-06: only the current lease holder writes; a stale token is refused by the runner itself.
#[test]
fn ac06_stale_lease_is_rejected_by_supervisor_and_runner() {
    let (mut sup, mut runner) = setup("esp32s3");
    let runner_id = RunnerId::new(RUNNER);
    let (a, b) = (TaskId::new("a"), TaskId::new("b"));

    let lease_a = sup
        .acquire_lease(&device(), &a, &runner_id, 100, 0)
        .unwrap();
    assert!(matches!(
        sup.acquire_lease(&device(), &b, &runner_id, 100, 10),
        Err(LeaseError::Held { .. })
    ));
    let op_a = approved_flash(&mut sup, "a", "esp32s3", "sha-a");
    let stale_request = sup.begin_dispatch(&op_a, &lease_a, 50).unwrap();

    // Task A stalls past its lease; B takes over and flashes.
    let lease_b = sup
        .acquire_lease(&device(), &b, &runner_id, 100, 150)
        .unwrap();
    assert!(lease_b.fencing_token > lease_a.fencing_token);
    let op_b = approved_flash(&mut sup, "b", "esp32s3", "sha-b");
    assert_eq!(
        sup.dispatch(&op_b, &lease_b, &mut runner, 151).unwrap(),
        OperationState::Completed
    );

    // A's request finally reaches the runner: refused without touching the device.
    let outcome = runner.execute(&stale_request);
    assert_eq!(
        outcome,
        RunnerOutcome::Rejected(RunnerRejection::StaleFencingToken {
            presented: lease_a.fencing_token,
            highest_seen: lease_b.fencing_token,
        })
    );
    sup.complete(&op_a, outcome, 152).unwrap();
    assert_eq!(sup.record(&op_a).unwrap().state, OperationState::Rejected);

    // A new dispatch with A's old lease is refused by the supervisor.
    let op_a2 = approved_flash(&mut sup, "a", "esp32s3", "sha-a");
    assert!(matches!(
        sup.begin_dispatch(&op_a2, &lease_a, 160).unwrap_err(),
        SupervisorError::Lease(LeaseError::Superseded { .. })
    ));

    let dev = runner.device(&device()).unwrap();
    assert_eq!(dev.flash_writes, 1);
    assert_eq!(dev.image_sha256.as_deref(), Some("sha-b"));
}

/// A caller that skips the supervisor and forges a token below the runner's high-water mark is refused.
#[test]
fn runner_refuses_forged_low_token() {
    let (mut sup, mut runner) = setup("esp32s3");
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();
    sup.dispatch(&op, &lease, &mut runner, 1).unwrap();

    let forged = DispatchRequest {
        operation_id: OperationId::new("forged"),
        task_id: TaskId::new("agent"),
        fencing_token: FencingToken(0),
        parameters_digest: "x".into(),
        params: flash_params("esp32s3", "evil"),
    };
    assert!(matches!(
        runner.execute(&forged),
        RunnerOutcome::Rejected(RunnerRejection::StaleFencingToken { .. })
    ));
    assert_eq!(runner.device(&device()).unwrap().flash_writes, 1);
}

/// AC-10: an S3 artifact sent to a C3 device is refused at ChipCheck.
#[test]
fn ac10_chip_mismatch_is_rejected_before_write() {
    let (mut sup, mut runner) = setup("esp32c3");
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();

    assert_eq!(
        sup.dispatch(&op, &lease, &mut runner, 1).unwrap(),
        OperationState::Rejected
    );
    assert_eq!(
        sup.record(&op).unwrap().rejection,
        Some(RunnerRejection::ChipMismatch {
            expected: "esp32s3".into(),
            detected: "esp32c3".into(),
        })
    );
    assert_eq!(runner.device(&device()).unwrap().flash_writes, 0);
    assert!(
        sup.audit()
            .iter()
            .any(|e| e.kind == "op.complete" && e.detail == "Rejected")
    );
}

#[test]
fn revision_outside_profile_range_is_rejected() {
    let mut runner = MockRunner::new(RUNNER);
    runner.add_device(&device(), "esp32p4", ChipRevision::new(1, 0));
    let mut sup = Supervisor::default();

    let mut params = flash_params("esp32p4", "sha-a");
    params.target.as_mut().unwrap().revisions = RevisionPolicy {
        min: Some(ChipRevision::new(3, 0)),
        max: None,
        notes: None,
    };
    let op = sup.prepare(intent("t1", params), 0).unwrap();
    let approval = approve(&sup, &op, 0);
    sup.authorize(&op, approval, 0).unwrap();
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();

    assert_eq!(
        sup.dispatch(&op, &lease, &mut runner, 1).unwrap(),
        OperationState::Rejected
    );
    assert!(matches!(
        sup.record(&op).unwrap().rejection,
        Some(RunnerRejection::RevisionOutOfRange { .. })
    ));
}

/// AC-13: cancel during flash is deferred; a confirmed force-abort yields OutcomeUnknown, no reflash.
#[test]
fn ac13_cancel_during_flash() {
    let (mut sup, mut runner) = setup("esp32s3");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();

    // Cancel before dispatch: never runs.
    let early = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    assert_eq!(
        sup.request_cancel(&early, 1).unwrap(),
        CancelOutcome::Cancelled
    );
    assert!(sup.begin_dispatch(&early, &lease, 2).is_err());

    // Cancel while writing: deferred, write completes, flag recorded.
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let request = sup.begin_dispatch(&op, &lease, 3).unwrap();
    assert_eq!(
        sup.request_cancel(&op, 4).unwrap(),
        CancelOutcome::DeferredUntilComplete
    );
    let outcome = runner.execute(&request);
    assert_eq!(
        sup.complete(&op, outcome, 5).unwrap(),
        OperationState::Completed
    );
    assert!(sup.record(&op).unwrap().cancel_requested);

    // Forced abort needs confirmation and leaves the result unknown.
    let forced = approved_flash(&mut sup, "t1", "esp32s3", "sha-c");
    sup.begin_dispatch(&forced, &lease, 6).unwrap();
    assert_eq!(
        sup.force_abort(&forced, false, 7).unwrap_err(),
        SupervisorError::ConfirmationRequired
    );
    sup.force_abort(&forced, true, 7).unwrap();
    assert_eq!(
        sup.record(&forced).unwrap().state,
        OperationState::OutcomeUnknown
    );
    assert!(sup.begin_dispatch(&forced, &lease, 8).is_err());
    assert_eq!(runner.device(&device()).unwrap().flash_writes, 1);
}

#[test]
fn failed_flash_is_final_and_needs_a_new_intent() {
    let (mut sup, mut runner) = setup("esp32s3");
    runner.inject(Fault::Fail);
    let op = approved_flash(&mut sup, "t1", "esp32s3", "sha-a");
    let lease = sup
        .acquire_lease(
            &device(),
            &TaskId::new("t1"),
            &RunnerId::new(RUNNER),
            60_000,
            0,
        )
        .unwrap();
    assert_eq!(
        sup.dispatch(&op, &lease, &mut runner, 1).unwrap(),
        OperationState::Failed
    );
    assert!(sup.dispatch(&op, &lease, &mut runner, 2).is_err());
    assert_eq!(runner.device(&device()).unwrap().flash_writes, 0);
}
