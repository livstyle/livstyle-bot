use serde_json::json;

use super::*;

fn store_with_task() -> Store {
    let mut s = Store::open_in_memory().unwrap();
    s.create_task("t1", "draft", &json!({ "requirement": "blink" }))
        .unwrap();
    s
}

#[test]
fn optimistic_concurrency() {
    let mut s = store_with_task();
    let v = s.task::<Value>("t1").unwrap();
    assert_eq!(v.version, 1);
    let v2 = s
        .update_task(
            "t1",
            1,
            "planning",
            &json!({ "requirement": "blink fast" }),
            None,
        )
        .unwrap();
    assert_eq!(v2, 2);
    // A writer holding the stale version loses.
    let err = s
        .update_task("t1", 1, "planning", &json!({}), None)
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::Conflict {
            expected: 1,
            actual: 2,
            ..
        }
    ));
    assert_eq!(
        s.task::<Value>("t1").unwrap().value["requirement"],
        "blink fast"
    );
    assert!(matches!(
        s.create_task("t1", "draft", &json!({})),
        Err(StoreError::Exists { .. })
    ));
}

#[test]
fn plans_are_append_only_revisions() {
    let mut s = store_with_task();
    assert!(s.latest_plan::<Value>("t1").unwrap().is_none());
    assert_eq!(
        s.save_plan("t1", &json!({ "nodes": 3 }), "initial")
            .unwrap(),
        1
    );
    assert_eq!(
        s.save_plan("t1", &json!({ "nodes": 4 }), "amend").unwrap(),
        2
    );
    let latest = s.latest_plan::<Value>("t1").unwrap().unwrap();
    assert_eq!(
        (latest.version, latest.value["nodes"].as_i64()),
        (2, Some(4))
    );
}

#[test]
fn interventions_survive_until_settled() {
    let mut s = store_with_task();
    assert_eq!(s.record_intervention("i1", "t1", "改成 C6").unwrap(), 1);
    assert_eq!(s.record_intervention("i2", "t1", "再快一点").unwrap(), 2);
    s.update_intervention(
        "i1",
        "applied",
        Some(&json!({ "intent": "amend_requirement" })),
        None::<&Value>,
    )
    .unwrap();
    let pending = s.pending_interventions("t1").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, "i2");
    assert_eq!(pending[0].3, "received");
}

#[test]
fn events_are_ordered_and_resumable() {
    let mut s = store_with_task();
    s.save_plan("t1", &json!({}), "initial").unwrap();
    s.record_intervention("i1", "t1", "x").unwrap();
    let all = s.events_after("t1", 0).unwrap();
    let kinds: Vec<_> = all.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["task.created", "plan.updated", "intervention.received"]
    );
    let tail = s.events_after("t1", all[0].seq).unwrap();
    assert_eq!(tail.len(), 2);
}

#[test]
fn operation_lifecycle_is_enforced() {
    let mut s = store_with_task();
    s.record_operation("op1", "t1", "flash", "abc", "dev1")
        .unwrap();
    // Cannot skip authorization.
    assert!(matches!(
        s.transition_operation("op1", OperationState::Dispatched, Some(7), &Value::Null),
        Err(StoreError::BadOperationState { .. })
    ));
    s.transition_operation("op1", OperationState::Authorized, None, &Value::Null)
        .unwrap();
    s.transition_operation("op1", OperationState::Dispatched, Some(7), &Value::Null)
        .unwrap();
    s.transition_operation(
        "op1",
        OperationState::Succeeded,
        None,
        &json!({ "verified": true }),
    )
    .unwrap();
    let op = s.operation("op1").unwrap();
    assert_eq!(op.state, OperationState::Succeeded);
    assert_eq!(op.fencing_token, Some(7));
    assert!(
        s.transition_operation("op1", OperationState::Failed, None, &Value::Null)
            .is_err()
    );
}

#[test]
fn restart_marks_dispatched_operations_outcome_unknown() {
    let dir = std::env::temp_dir().join(format!("livstyle-store-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.db");
    let _ = std::fs::remove_file(&path);
    {
        let mut s = Store::open(&path).unwrap();
        s.create_task("t1", "running", &json!({})).unwrap();
        s.record_operation("op1", "t1", "flash", "abc", "dev1")
            .unwrap();
        s.transition_operation("op1", OperationState::Authorized, None, &Value::Null)
            .unwrap();
        s.transition_operation("op1", OperationState::Dispatched, Some(3), &Value::Null)
            .unwrap();
        s.record_operation("op2", "t1", "build", "def", "host")
            .unwrap();
        s.upsert_runtime_mapping(&RuntimeMapping {
            task_id: "t1".into(),
            runtime: "cursor".into(),
            kind: "run".into(),
            external_id: "run-1".into(),
            resume_offset: Some("o-5".into()),
        })
        .unwrap();
        // Process "crashes" here: dropped without completing op1.
    }
    let mut s = Store::open(&path).unwrap();
    let recovered = s.recover_inflight_operations().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, "op1");
    assert_eq!(recovered[0].state, OperationState::OutcomeUnknown);
    // Not re-dispatchable; only reconciliation may settle it.
    assert!(
        s.transition_operation("op1", OperationState::Dispatched, None, &Value::Null)
            .is_err()
    );
    s.transition_operation(
        "op1",
        OperationState::Succeeded,
        None,
        &json!({ "reconciled": "readback" }),
    )
    .unwrap();
    assert_eq!(s.operation("op2").unwrap().state, OperationState::Prepared);
    assert_eq!(
        s.runtime_mappings("t1").unwrap()[0]
            .resume_offset
            .as_deref(),
        Some("o-5")
    );
    assert_eq!(
        s.verify_audit_chain().unwrap(),
        s.audit_entries().unwrap().len()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn audit_is_append_only_and_chained() {
    let mut s = store_with_task();
    s.audit(
        "user",
        "approval.granted",
        "op1",
        &json!({ "digest": "abc" }),
    )
    .unwrap();
    s.audit(
        "supervisor",
        "lease.acquired",
        "dev1",
        &json!({ "token": 1 }),
    )
    .unwrap();
    assert_eq!(s.verify_audit_chain().unwrap(), 2);
    assert!(
        s.conn()
            .execute("UPDATE audit SET actor = 'x' WHERE seq = 1", [])
            .is_err()
    );
    assert!(
        s.conn()
            .execute("DELETE FROM audit WHERE seq = 2", [])
            .is_err()
    );
}

#[test]
fn refuses_future_schema() {
    let dir = std::env::temp_dir().join(format!("livstyle-store-future-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.db");
    let _ = std::fs::remove_file(&path);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::FutureSchema { found: 99 })
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}
