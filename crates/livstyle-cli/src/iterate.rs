use livstyle_domain::{BoardRuntime, ChangeKind, IterationStep, plan_iteration};

pub fn run(change: &str, base: &str) -> Result<(), String> {
    let change = match change {
        "app" => ChangeKind::AppLogic,
        "container" => ChangeKind::AppContainer,
        "base" => ChangeKind::BaseRuntime,
        "realtime" => ChangeKind::HardRealtime,
        "driver" => ChangeKind::NewDriver,
        "bringup" => ChangeKind::ChipBringUp,
        "sdkconfig" => ChangeKind::Sdkconfig,
        other => {
            return Err(format!(
                "未知改动 {other}，可用：app, container, base, realtime, driver, bringup, sdkconfig"
            ));
        }
    };
    let board = match base {
        "ready" => BoardRuntime::BaseReady,
        "none" => BoardRuntime::Unprovisioned,
        other => return Err(format!("未知底座状态 {other}，可用：ready, none")),
    };
    let plan = plan_iteration(change, board);
    println!("{}", plan.reason);
    for (index, step) in plan.steps.iter().enumerate() {
        println!("  {}. {}", index + 1, step_zh(*step));
    }
    Ok(())
}

fn step_zh(step: IterationStep) -> &'static str {
    match step {
        IterationStep::HostDryRun => "先在电脑上跑通这段逻辑",
        IterationStep::UsbFlashBase => "USB 刷一次底座（之后应用更新不再走 USB）",
        IterationStep::WifiReplaceContainer => "局域网替换应用容器，不复位、不重烧整镜像",
        IterationStep::IdfImage => "ESP-IDF 整镜像重新编译并用 USB 烧录",
    }
}
