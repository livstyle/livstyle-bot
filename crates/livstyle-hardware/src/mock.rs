//! Fault-injecting runner for Spike 3 (no real hardware).

use std::collections::{HashMap, VecDeque};

use livstyle_profiles::ChipRevision;

use crate::{
    DetectedChip, DeviceId, DispatchRequest, FenceGuard, OperationId, OperationReceipt,
    OperationType, Runner, RunnerId, RunnerOutcome, RunnerRejection, chip_check,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// esptool exits non-zero; the device is untouched.
    Fail,
    /// Transport drops before any byte is written.
    DisconnectBeforeWrite,
    /// Transport drops after the image is written.
    DisconnectAfterWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockDevice {
    pub chip: DetectedChip,
    pub image_sha256: Option<String>,
    pub flash_writes: u32,
}

#[derive(Debug)]
pub struct MockRunner {
    id: RunnerId,
    fence: FenceGuard,
    devices: HashMap<DeviceId, MockDevice>,
    faults: VecDeque<Fault>,
    pub executed: Vec<OperationId>,
}

impl MockRunner {
    pub fn new(id: &str) -> Self {
        Self {
            id: RunnerId::new(id),
            fence: FenceGuard::default(),
            devices: HashMap::new(),
            faults: VecDeque::new(),
            executed: Vec::new(),
        }
    }

    pub fn add_device(&mut self, id: &DeviceId, chip: &str, revision: ChipRevision) {
        self.devices.insert(
            id.clone(),
            MockDevice {
                chip: DetectedChip {
                    chip: chip.to_owned(),
                    revision,
                },
                image_sha256: None,
                flash_writes: 0,
            },
        );
    }

    /// Faults are consumed in order by subsequent flash operations.
    pub fn inject(&mut self, fault: Fault) {
        self.faults.push_back(fault);
    }

    pub fn device(&self, id: &DeviceId) -> Option<&MockDevice> {
        self.devices.get(id)
    }

    fn receipt(
        &self,
        req: &DispatchRequest,
        chip: &DetectedChip,
        exit: i32,
        out: &str,
    ) -> OperationReceipt {
        OperationReceipt {
            operation_id: req.operation_id.clone(),
            runner_id: self.id.clone(),
            fencing_token: req.fencing_token,
            command_summary: format!("mock {:?}", req.params.operation_type),
            exit_status: Some(exit),
            output_summary: out.to_owned(),
            detected_chip: Some(chip.clone()),
        }
    }
}

impl Runner for MockRunner {
    fn runner_id(&self) -> &RunnerId {
        &self.id
    }

    fn execute(&mut self, req: &DispatchRequest) -> RunnerOutcome {
        let params = &req.params;
        if params.runner_id != self.id {
            return RunnerOutcome::Rejected(RunnerRejection::WrongRunner {
                expected: self.id.clone(),
            });
        }
        if let Err(rejection) = self.fence.admit(&params.device_id, req.fencing_token) {
            return RunnerOutcome::Rejected(rejection);
        }
        let Some(device) = self.devices.get(&params.device_id) else {
            return RunnerOutcome::Rejected(RunnerRejection::DeviceNotFound);
        };
        let chip = device.chip.clone();
        if params.operation_type.writes_device() {
            if let Err(rejection) = chip_check(params.target.as_ref(), &chip) {
                return RunnerOutcome::Rejected(rejection);
            }
        }
        self.executed.push(req.operation_id.clone());

        if params.operation_type != OperationType::Flash {
            return RunnerOutcome::Completed(self.receipt(req, &chip, 0, "ok"));
        }

        let fault = self.faults.pop_front();
        let write = |runner: &mut Self| {
            let device = runner.devices.get_mut(&params.device_id).expect("checked");
            device.flash_writes += 1;
            device.image_sha256 = params.artifact_sha256.clone();
        };
        match fault {
            None => {
                write(self);
                RunnerOutcome::Completed(self.receipt(req, &chip, 0, "Hash of data verified."))
            }
            Some(Fault::Fail) => {
                RunnerOutcome::Failed(self.receipt(req, &chip, 2, "A fatal error occurred"))
            }
            Some(Fault::DisconnectBeforeWrite) => RunnerOutcome::OutcomeUnknown {
                detail: "serial port disappeared".into(),
            },
            Some(Fault::DisconnectAfterWrite) => {
                write(self);
                RunnerOutcome::OutcomeUnknown {
                    detail: "serial port disappeared".into(),
                }
            }
        }
    }
}
