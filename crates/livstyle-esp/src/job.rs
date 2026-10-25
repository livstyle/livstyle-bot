//! Bind a real flasher-args digest to one approval, then dispatch.
//! A mismatched digest never reaches the runner.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use livstyle_domain::TaskId;
use livstyle_hardware::{
    Approval, ApprovalId, DeviceId, Millis, OperationIntent, OperationParams, OperationType,
    Runner, RunnerId, RunnerOutcome, Supervisor, TargetChip,
};
use livstyle_profiles::RevisionPolicy;

use crate::FlasherArgs;

pub fn boot_log_acceptable(log: &str) -> Result<(), String> {
    if log.trim().is_empty() {
        return Err("启动日志是空的".into());
    }
    let lower = log.to_ascii_lowercase();
    if lower.contains("panic") || lower.contains("guru meditation") {
        return Err("启动日志里有 panic".into());
    }
    Ok(())
}

pub struct FlashOffer {
    pub digest: String,
    pub chip: String,
    pub params: OperationParams,
}

pub fn flash_offer(build_dir: &Path, device_id: &str, chip: &str) -> Result<FlashOffer, String> {
    let args = FlasherArgs::load(build_dir)?;
    if !args.chip.is_empty() && args.chip != chip && !args.chip.eq_ignore_ascii_case(chip) {
        return Err(format!(
            "构建产物是 {}，识别到的芯片是 {chip}",
            args.chip
        ));
    }
    let artifact = args.artifact_sha256();
    let params = OperationParams {
        operation_type: OperationType::Flash,
        device_id: DeviceId::new(device_id),
        runner_id: RunnerId::new("esp"),
        target: Some(TargetChip {
            chip: chip.to_string(),
            revisions: RevisionPolicy::default(),
            chip_profile_version: "draft".into(),
        }),
        board_profile_id: None,
        artifact_sha256: Some(artifact),
        regions: args
            .files
            .iter()
            .map(|file| livstyle_hardware::FlashRegion {
                offset: file.offset,
                sha256: file.sha256.clone(),
            })
            .collect(),
        extra: Default::default(),
    };
    let digest = params.digest();
    Ok(FlashOffer {
        digest,
        chip: chip.to_string(),
        params,
    })
}

/// Dispatch only when `approved_digest` is the offer's digest.
pub fn commit_flash(
    runner: &mut impl Runner,
    offer: &FlashOffer,
    approved_digest: &str,
    task_id: &str,
) -> Result<String, String> {
    if approved_digest != offer.digest {
        return Err("审批摘要和当前产物不一致，没有烧录".into());
    }
    if runner.runner_id() != &offer.params.runner_id {
        return Err("运行器和审批时的摘要不一致，没有烧录".into());
    }
    let params = offer.params.clone();
    let digest = offer.digest.clone();
    let now = now_ms();
    let mut supervisor = Supervisor::default();
    let task = TaskId::new(task_id);
    let lease = supervisor
        .acquire_lease(&params.device_id, &task, &params.runner_id, 60_000, now)
        .map_err(|e| e.to_string())?;
    let intent = OperationIntent {
        task_id: task,
        params,
    };
    let op = supervisor
        .prepare(intent, now)
        .map_err(|e| e.to_string())?;
    let approval = Approval {
        approval_id: ApprovalId::new(format!("ui-{op}")),
        operation_type: OperationType::Flash,
        risk: OperationType::Flash.risk(),
        parameters_digest: digest.clone(),
        approved_by: "livstyle-ui".into(),
        granted_at: now,
        expires_at: now + 60_000,
    };
    supervisor
        .authorize(&op, approval, now)
        .map_err(|e| e.to_string())?;
    let request = supervisor
        .begin_dispatch(&op, &lease, now)
        .map_err(|e| e.to_string())?;
    let outcome = runner.execute(&request);
    let state = supervisor
        .complete(&op, outcome, now)
        .map_err(|e| e.to_string())?;
    match state {
        livstyle_hardware::OperationState::Completed => Ok(digest),
        other => Err(format!("烧录没有完成：{other:?}")),
    }
}

pub fn write_evidence(root: &Path, task: &str, run: &str, log: &str, digest: &str) -> Result<PathBuf, String> {
    boot_log_acceptable(log)?;
    let dir = root.join(task).join(run);
    fs::create_dir_all(dir.join("logs")).map_err(|e| e.to_string())?;
    fs::write(dir.join("logs/boot.log"), log).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "task": task,
        "run": run,
        "artifact_sha256": digest,
        "boot_log": "logs/boot.log",
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(dir)
}

fn now_ms() -> Millis {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use livstyle_hardware::mock::MockRunner;
    use livstyle_hardware::{DeviceId, RunnerId};
    use livstyle_profiles::ChipRevision;

    #[test]
    fn empty_or_panic_log_is_rejected() {
        assert!(boot_log_acceptable("").is_err());
        assert!(boot_log_acceptable("Guru Meditation Error").is_err());
        assert!(boot_log_acceptable("app started\n").is_ok());
    }

    #[test]
    fn mismatched_digest_does_not_write() {
        let mut params = OperationParams {
            operation_type: OperationType::Flash,
            device_id: DeviceId::new("esp-1"),
            runner_id: RunnerId::new("mock"),
            target: Some(TargetChip {
                chip: "esp32c3".into(),
                revisions: RevisionPolicy::default(),
                chip_profile_version: "draft".into(),
            }),
            board_profile_id: None,
            artifact_sha256: Some("abc".into()),
            regions: vec![],
            extra: Default::default(),
        };
        let digest = params.digest();
        params.runner_id = RunnerId::new("mock");
        let offer = FlashOffer {
            digest: params.digest(),
            chip: "esp32c3".into(),
            params,
        };
        let mut runner = MockRunner::new("mock");
        runner.add_device(&DeviceId::new("esp-1"), "esp32c3", ChipRevision::new(0, 1));
        let err = commit_flash(&mut runner, &offer, "not-the-digest", "task-1").unwrap_err();
        assert!(err.contains("没有烧录"));
        assert_eq!(runner.device(&DeviceId::new("esp-1")).unwrap().flash_writes, 0);
        assert_ne!(digest, "not-the-digest");
    }
}
