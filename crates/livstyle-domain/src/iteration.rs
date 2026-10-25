//! ESP32-family inner loop, following Toit + Jaguar.
//!
//! USB flashing is the provisioning step, not the edit loop. Once a base
//! runtime is on the board, application changes go over the LAN as a
//! replaceable container. Hard realtime, new drivers and chip bring-up stay
//! on a full ESP-IDF image.

/// What the user just changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Ordinary application logic.
    AppLogic,
    /// Add or replace one service container (NTP, a sensor driver package).
    AppContainer,
    /// The base runtime / VM itself.
    BaseRuntime,
    /// Microsecond-level control. A GC runtime is the wrong tool.
    HardRealtime,
    /// No packaged driver exists yet.
    NewDriver,
    /// New chip or board bring-up.
    ChipBringUp,
    /// sdkconfig / flash layout / PSRAM. Invalidates the base image.
    Sdkconfig,
}

/// Whether the board already has the base runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardRuntime {
    Unprovisioned,
    BaseReady,
}

/// One delivery step. Order in [`IterationPlan::steps`] is the order to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationStep {
    /// Run the same logic on the computer before touching the board.
    HostDryRun,
    /// USB, once: flash the base runtime. Risk R2, not interruptible.
    UsbFlashBase,
    /// LAN push of one container. Does not reset the chip or rewrite the base.
    WifiReplaceContainer,
    /// Full ESP-IDF image: rebuild and USB flash. Risk R2.
    IdfImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IterationPlan {
    pub steps: Vec<IterationStep>,
    pub reason: &'static str,
}

impl IterationPlan {
    pub fn uses_usb(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s, IterationStep::UsbFlashBase | IterationStep::IdfImage))
    }
}

/// Picks the delivery path. Application work never starts with a full-image
/// reflash when the base is already on the device.
pub fn plan_iteration(change: ChangeKind, board: BoardRuntime) -> IterationPlan {
    use ChangeKind::*;
    use IterationStep::*;
    match change {
        HardRealtime | NewDriver | ChipBringUp | Sdkconfig => IterationPlan {
            steps: vec![IdfImage],
            reason: "硬实时、新驱动、芯片bring-up 或 sdkconfig 走完整 ESP-IDF 镜像，不用 VM 容器",
        },
        BaseRuntime => IterationPlan {
            steps: vec![UsbFlashBase],
            reason: "底座运行时变了，才再次使用 USB；应用容器不用跟着重刷",
        },
        AppLogic | AppContainer => match board {
            BoardRuntime::Unprovisioned => IterationPlan {
                steps: vec![HostDryRun, UsbFlashBase, WifiReplaceContainer],
                reason: "板子还没有底座：先在电脑上跑通，USB 只刷一次底座，之后改为局域网热替换",
            },
            BoardRuntime::BaseReady => IterationPlan {
                steps: vec![HostDryRun, WifiReplaceContainer],
                reason: "底座已在板上：保存后经局域网替换应用容器，不再拔 USB 重烧整镜像",
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_board_skips_usb_for_app_edits() {
        let plan = plan_iteration(ChangeKind::AppLogic, BoardRuntime::BaseReady);
        assert!(!plan.uses_usb());
        assert_eq!(
            plan.steps,
            vec![
                IterationStep::HostDryRun,
                IterationStep::WifiReplaceContainer
            ]
        );
    }

    #[test]
    fn first_board_uses_usb_only_to_provision() {
        let plan = plan_iteration(ChangeKind::AppContainer, BoardRuntime::Unprovisioned);
        assert_eq!(
            plan.steps,
            vec![
                IterationStep::HostDryRun,
                IterationStep::UsbFlashBase,
                IterationStep::WifiReplaceContainer
            ]
        );
    }

    #[test]
    fn realtime_and_sdkconfig_stay_on_idf() {
        for change in [
            ChangeKind::HardRealtime,
            ChangeKind::NewDriver,
            ChangeKind::ChipBringUp,
            ChangeKind::Sdkconfig,
        ] {
            let plan = plan_iteration(change, BoardRuntime::BaseReady);
            assert_eq!(plan.steps, vec![IterationStep::IdfImage]);
        }
    }
}
