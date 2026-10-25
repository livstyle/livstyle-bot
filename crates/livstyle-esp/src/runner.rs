use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use livstyle_hardware::{
    DetectedChip, DeviceId, DispatchRequest, FenceGuard, OperationReceipt, OperationType, Runner,
    RunnerId, RunnerOutcome, RunnerRejection, chip_check,
};

use crate::{Exec, FlasherArgs, parse_chip_info};

/// Device id → serial port, refreshed from [`crate::enumerate`].
pub type PortMap = HashMap<DeviceId, PathBuf>;

/// `(chip, command summary, output)`.
type Detected = (DetectedChip, String, String);
/// `(command summary, output, exit status)`.
type DetectFailure = (String, String, Option<i32>);

const DETECT_TIMEOUT: Duration = Duration::from_secs(30);
const FLASH_TIMEOUT: Duration = Duration::from_secs(300);

/// esptool command spelling. esptool v5 (ESP-IDF 6) renamed sub-commands and
/// options to kebab-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    V4,
    V5,
}

impl Dialect {
    fn w(self, kebab: &str) -> String {
        match self {
            Dialect::V5 => kebab.to_owned(),
            Dialect::V4 => kebab.replace('-', "_"),
        }
    }
}

/// [`Runner`] backed by esptool from the selected ESP-IDF Python environment.
///
/// Before any write it re-checks, independently of the supervisor: the
/// fencing token, the runner id, the detected chip and revision (ChipCheck),
/// and that the image files still hash to the approved artifact.
pub struct EspRunner {
    id: RunnerId,
    fence: FenceGuard,
    ports: PortMap,
    exec: Box<dyn Exec>,
    program: PathBuf,
    base_args: Vec<String>,
    dialect: Dialect,
    pub baud: u32,
}

impl EspRunner {
    pub fn new(
        id: &str,
        program: PathBuf,
        base_args: Vec<String>,
        dialect: Dialect,
        exec: Box<dyn Exec>,
    ) -> Self {
        Self {
            id: RunnerId::new(id),
            fence: FenceGuard::default(),
            ports: PortMap::new(),
            exec,
            program,
            base_args,
            dialect,
            baud: 460_800,
        }
    }

    /// Uses the IDF install's Python and esptool dialect.
    pub fn for_install(id: &str, install: &crate::IdfInstall, exec: Box<dyn Exec>) -> Self {
        let (program, base) = install.esptool();
        let dialect = if install.version.major >= 6 {
            Dialect::V5
        } else {
            Dialect::V4
        };
        Self::new(id, program, base, dialect, exec)
    }

    pub fn set_ports(&mut self, ports: PortMap) {
        self.ports = ports;
    }

    /// Reads the chip type. This resets the board into download mode and back.
    pub fn read_chip_line(&self, port: &std::path::Path) -> Result<String, String> {
        match self.detect(&port.to_string_lossy(), "hard-reset") {
            Ok((chip, _, _)) => Ok(format!("{}，版本 {}", chip.chip, chip.revision)),
            Err((_, out, _)) => Err(out),
        }
    }

    fn args(&self, port: &str, before: &str, after: &str, rest: &[String]) -> Vec<String> {
        let d = self.dialect;
        let mut a = self.base_args.clone();
        a.extend([
            "-p".into(),
            port.into(),
            "-b".into(),
            self.baud.to_string(),
            "--before".into(),
            d.w(before),
            "--after".into(),
            d.w(after),
        ]);
        a.extend(rest.iter().cloned());
        a
    }

    fn summary(&self, args: &[String]) -> String {
        format!("{} {}", self.program.display(), args.join(" "))
    }

    fn receipt(
        &self,
        req: &DispatchRequest,
        cmd: String,
        status: Option<i32>,
        output: &str,
        chip: Option<DetectedChip>,
    ) -> OperationReceipt {
        let tail: String = {
            let lines: Vec<&str> = output.lines().rev().take(20).collect();
            lines.into_iter().rev().collect::<Vec<_>>().join("\n")
        };
        OperationReceipt {
            operation_id: req.operation_id.clone(),
            runner_id: self.id.clone(),
            fencing_token: req.fencing_token,
            command_summary: cmd,
            exit_status: status,
            output_summary: tail,
            detected_chip: chip,
        }
    }

    /// Connects in download mode and reads chip type and revision.
    fn detect(&self, port: &str, after: &str) -> Result<Detected, DetectFailure> {
        let args = self.args(port, "default-reset", after, &[self.dialect.w("chip-id")]);
        let cmd = self.summary(&args);
        match self.exec.run(&self.program, &args, DETECT_TIMEOUT) {
            Ok(out) if out.success() => match parse_chip_info(&out.combined()) {
                Some(chip) => Ok((chip, cmd, out.combined())),
                None => Err((
                    cmd,
                    format!("could not parse chip info:\n{}", out.combined()),
                    out.status,
                )),
            },
            Ok(out) => Err((cmd, out.combined(), out.status)),
            Err(e) => Err((cmd, e.to_string(), None)),
        }
    }

    fn flash(&mut self, req: &DispatchRequest, port: &str) -> RunnerOutcome {
        let p = &req.params;
        let Some(build_dir) = p.extra.get("build_dir") else {
            return RunnerOutcome::Rejected(RunnerRejection::Unsupported(
                "flash needs extra.build_dir".into(),
            ));
        };
        let artifact = match FlasherArgs::load(std::path::Path::new(build_dir)) {
            Ok(a) => a,
            Err(e) => return RunnerOutcome::Rejected(RunnerRejection::Unsupported(e)),
        };
        let actual = artifact.artifact_sha256();
        let expected = p.artifact_sha256.clone().unwrap_or_default();
        if actual != expected {
            return RunnerOutcome::Rejected(RunnerRejection::ArtifactMismatch { expected, actual });
        }
        if let Some(target) = &p.target {
            if target.chip != artifact.chip {
                return RunnerOutcome::Rejected(RunnerRejection::ChipMismatch {
                    expected: target.chip.clone(),
                    detected: format!("{} (artifact)", artifact.chip),
                });
            }
        }

        // ChipCheck. Stay in the bootloader so the write connects immediately.
        let (detected, _, _) = match self.detect(port, "no-reset") {
            Ok(d) => d,
            Err((cmd, out, status)) => {
                return RunnerOutcome::Failed(self.receipt(req, cmd, status, &out, None));
            }
        };
        if let Err(rejection) = chip_check(p.target.as_ref(), &detected) {
            return RunnerOutcome::Rejected(rejection);
        }

        let mut rest = vec![self.dialect.w("write-flash")];
        rest.extend(artifact.write_flash_args.iter().cloned());
        for f in &artifact.files {
            rest.push(format!("0x{:x}", f.offset));
            rest.push(f.path.to_string_lossy().into_owned());
        }
        let mut args = self.args(port, "default-reset", "hard-reset", &rest);
        // `--chip` is a global option and must precede the sub-command.
        args.splice(
            self.base_args.len()..self.base_args.len(),
            ["--chip".to_owned(), artifact.chip.clone()],
        );
        let cmd = self.summary(&args);
        match self.exec.run(&self.program, &args, FLASH_TIMEOUT) {
            Ok(out) if out.success() => RunnerOutcome::Completed(self.receipt(
                req,
                cmd,
                out.status,
                &out.combined(),
                Some(detected),
            )),
            Ok(out) => {
                let text = out.combined();
                let started_writing = text.contains("Writing at")
                    || text.contains("Erasing")
                    || text.contains("Compressed ");
                if out.timed_out || started_writing {
                    RunnerOutcome::OutcomeUnknown {
                        detail: format!(
                            "esptool {} after writing started; device contents unknown\n{}",
                            if out.timed_out {
                                "timed out".into()
                            } else {
                                format!("exited {:?}", out.status)
                            },
                            self.receipt(req, cmd, out.status, &text, None)
                                .output_summary
                        ),
                    }
                } else {
                    RunnerOutcome::Failed(self.receipt(req, cmd, out.status, &text, Some(detected)))
                }
            }
            Err(e) => RunnerOutcome::OutcomeUnknown {
                detail: format!("esptool could not be supervised: {e}"),
            },
        }
    }
}

impl Runner for EspRunner {
    fn runner_id(&self) -> &RunnerId {
        &self.id
    }

    fn execute(&mut self, req: &DispatchRequest) -> RunnerOutcome {
        let p = &req.params;
        if p.runner_id != self.id {
            return RunnerOutcome::Rejected(RunnerRejection::WrongRunner {
                expected: self.id.clone(),
            });
        }
        if let Err(r) = self.fence.admit(&p.device_id, req.fencing_token) {
            return RunnerOutcome::Rejected(r);
        }
        let Some(port) = self.ports.get(&p.device_id).filter(|p| p.exists()) else {
            return RunnerOutcome::Rejected(RunnerRejection::DeviceNotFound);
        };
        let port = port.to_string_lossy().into_owned();
        match p.operation_type {
            OperationType::ReadChipInfo => match self.detect(&port, "hard-reset") {
                Ok((chip, cmd, out)) => {
                    RunnerOutcome::Completed(self.receipt(req, cmd, Some(0), &out, Some(chip)))
                }
                Err((cmd, out, status)) => {
                    RunnerOutcome::Failed(self.receipt(req, cmd, status, &out, None))
                }
            },
            OperationType::Reset => match self.detect(&port, "hard-reset") {
                Ok((chip, cmd, out)) => {
                    RunnerOutcome::Completed(self.receipt(req, cmd, Some(0), &out, Some(chip)))
                }
                Err((cmd, out, status)) => {
                    RunnerOutcome::Failed(self.receipt(req, cmd, status, &out, None))
                }
            },
            OperationType::ReadLog => {
                let secs = p
                    .extra
                    .get("seconds")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5u64);
                let baud = p
                    .extra
                    .get("baud")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(115_200u32);
                let cmd = format!("read_log {port} {baud} {secs}s");
                match crate::serial::read_log(
                    std::path::Path::new(&port),
                    baud,
                    Duration::from_secs(secs),
                    256 * 1024,
                ) {
                    Ok(bytes) => RunnerOutcome::Completed(self.receipt(
                        req,
                        cmd,
                        Some(0),
                        &String::from_utf8_lossy(&bytes),
                        None,
                    )),
                    Err(e) => {
                        RunnerOutcome::Failed(self.receipt(req, cmd, None, &e.to_string(), None))
                    }
                }
            }
            OperationType::Flash => self.flash(req, &port),
            other => RunnerOutcome::Rejected(RunnerRejection::Unsupported(format!(
                "{other:?} is not available in this release"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use livstyle_domain::TaskId;
    use livstyle_hardware::{FencingToken, OperationId, OperationParams, TargetChip};
    use livstyle_profiles::{ChipRevision, RevisionPolicy};

    use super::*;
    use crate::CommandOutput;

    /// Scripted esptool: answers `chip_id`/`chip-id` with a chip line and
    /// writes with the given result. Records every invocation.
    #[derive(Clone)]
    struct FakeEsptool {
        chip_line: String,
        write: CommandOutput,
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl Exec for FakeEsptool {
        fn run(
            &self,
            _p: &std::path::Path,
            args: &[String],
            _t: Duration,
        ) -> std::io::Result<CommandOutput> {
            self.calls.lock().unwrap().push(args.to_vec());
            if args.iter().any(|a| a == "chip-id" || a == "chip_id") {
                Ok(CommandOutput {
                    status: Some(0),
                    stdout: format!("Connecting...\n{}\n", self.chip_line),
                    ..Default::default()
                })
            } else {
                Ok(self.write.clone())
            }
        }
    }

    struct Fixture {
        runner: EspRunner,
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        build: PathBuf,
        port: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.build.parent().unwrap());
        }
    }

    fn fixture(name: &str, chip_line: &str, write: CommandOutput) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("livstyle-esprunner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let build = root.join("build");
        crate::flasher::tests::fake_build(&build, "esp32c5");
        let port = root.join("ttyACM0");
        std::fs::write(&port, b"").unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let exec = FakeEsptool {
            chip_line: chip_line.into(),
            write,
            calls: calls.clone(),
        };
        let mut runner = EspRunner::new(
            "r1",
            "python".into(),
            vec!["-m".into(), "esptool".into()],
            Dialect::V5,
            Box::new(exec),
        );
        runner.set_ports(PortMap::from([(DeviceId::new("c5"), port.clone())]));
        Fixture {
            runner,
            calls,
            build,
            port,
        }
    }

    fn flash_req(f: &Fixture, token: u64, artifact: Option<String>) -> DispatchRequest {
        let artifact =
            artifact.unwrap_or_else(|| FlasherArgs::load(&f.build).unwrap().artifact_sha256());
        let params = OperationParams {
            operation_type: OperationType::Flash,
            device_id: DeviceId::new("c5"),
            runner_id: RunnerId::new("r1"),
            target: Some(TargetChip {
                chip: "esp32c5".into(),
                revisions: RevisionPolicy {
                    min: Some(ChipRevision::new(0, 1)),
                    max: None,
                    notes: None,
                },
                chip_profile_version: "1".into(),
            }),
            board_profile_id: Some("esp32c5-devkitc-1".into()),
            artifact_sha256: Some(artifact),
            regions: Vec::new(),
            extra: BTreeMap::from([("build_dir".into(), f.build.to_string_lossy().into_owned())]),
        };
        DispatchRequest {
            operation_id: OperationId::new(format!("op{token}")),
            task_id: TaskId::new("t"),
            fencing_token: FencingToken(token),
            parameters_digest: params.digest(),
            params,
        }
    }

    fn ok_write() -> CommandOutput {
        CommandOutput {
            status: Some(0),
            stdout:
                "Writing at 0x10000...\nHash of data verified.\nHard resetting via RTS pin...\n"
                    .into(),
            ..Default::default()
        }
    }

    #[test]
    fn flash_runs_chip_check_then_write_with_v5_spelling() {
        let mut f = fixture(
            "ok",
            "Chip type:          ESP32-C5 (QFN32) (revision v0.1)",
            ok_write(),
        );
        let req = flash_req(&f, 1, None);
        let outcome = f.runner.execute(&req);
        let RunnerOutcome::Completed(receipt) = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(receipt.detected_chip.unwrap().chip, "esp32c5");
        let calls = f.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(
            calls[0].contains(&"no-reset".to_owned()) && calls[0].contains(&"chip-id".to_owned())
        );
        let w = &calls[1];
        let pos = |s: &str| w.iter().position(|a| a == s).unwrap();
        assert!(pos("--chip") < pos("write-flash"), "{w:?}");
        assert!(w.contains(&"0x2000".to_owned()) && w.contains(&"--flash-mode".to_owned()));
        assert!(w.contains(&f.port.to_string_lossy().into_owned()));
    }

    #[test]
    fn wrong_chip_is_rejected_before_any_write() {
        let mut f = fixture(
            "wrong",
            "Chip type:          ESP32-C6 (QFN40) (revision v0.1)",
            ok_write(),
        );
        let outcome = f.runner.execute(&flash_req(&f, 1, None));
        assert!(
            matches!(
                outcome,
                RunnerOutcome::Rejected(RunnerRejection::ChipMismatch { .. })
            ),
            "{outcome:?}"
        );
        assert_eq!(f.calls.lock().unwrap().len(), 1, "only detection ran");
    }

    #[test]
    fn changed_artifact_is_rejected_without_touching_the_device() {
        let mut f = fixture("art", "Chip type: ESP32-C5 (revision v0.1)", ok_write());
        let req = flash_req(&f, 1, None);
        std::fs::write(f.build.join("app.bin"), b"tampered").unwrap();
        let outcome = f.runner.execute(&req);
        assert!(matches!(
            outcome,
            RunnerOutcome::Rejected(RunnerRejection::ArtifactMismatch { .. })
        ));
        assert!(f.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn stale_token_rejected_independently_of_supervisor() {
        let mut f = fixture("fence", "Chip type: ESP32-C5 (revision v0.1)", ok_write());
        assert!(matches!(
            f.runner.execute(&flash_req(&f, 5, None)),
            RunnerOutcome::Completed(_)
        ));
        let outcome = f.runner.execute(&flash_req(&f, 4, None));
        assert!(matches!(
            outcome,
            RunnerOutcome::Rejected(RunnerRejection::StaleFencingToken { .. })
        ));
    }

    #[test]
    fn failure_mid_write_is_outcome_unknown() {
        let mid = CommandOutput {
            status: Some(2),
            stdout: "Writing at 0x10000... (12 %)\n".into(),
            stderr: "A serial exception error occurred: device disconnected\n".into(),
            timed_out: false,
        };
        let mut f = fixture("mid", "Chip type: ESP32-C5 (revision v0.1)", mid);
        assert!(matches!(
            f.runner.execute(&flash_req(&f, 1, None)),
            RunnerOutcome::OutcomeUnknown { .. }
        ));

        let early = CommandOutput {
            status: Some(2),
            stderr: "A fatal error occurred: Failed to connect\n".into(),
            ..Default::default()
        };
        let mut f = fixture("early", "Chip type: ESP32-C5 (revision v0.1)", early);
        assert!(matches!(
            f.runner.execute(&flash_req(&f, 1, None)),
            RunnerOutcome::Failed(_)
        ));
    }

    #[test]
    fn missing_port_and_destructive_ops_rejected() {
        let mut f = fixture("port", "Chip type: ESP32-C5 (revision v0.1)", ok_write());
        let mut req = flash_req(&f, 1, None);
        std::fs::remove_file(&f.port).unwrap();
        assert!(matches!(
            f.runner.execute(&req),
            RunnerOutcome::Rejected(RunnerRejection::DeviceNotFound)
        ));
        std::fs::write(&f.port, b"").unwrap();
        req.params.operation_type = OperationType::EraseFlash;
        req.fencing_token = FencingToken(2);
        assert!(matches!(
            f.runner.execute(&req),
            RunnerOutcome::Rejected(RunnerRejection::Unsupported(_))
        ));
    }

    #[test]
    fn v4_dialect_uses_snake_case() {
        assert_eq!(Dialect::V4.w("write-flash"), "write_flash");
        assert_eq!(Dialect::V4.w("default-reset"), "default_reset");
        assert_eq!(Dialect::V5.w("chip-id"), "chip-id");
    }
}
