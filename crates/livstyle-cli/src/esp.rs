use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use livstyle_domain::TaskId;
use livstyle_esp::{EspRunner, PortMap, SystemExec, Toolchain, enumerate};
use livstyle_hardware::{
    DeviceId, DispatchRequest, FencingToken, OperationId, OperationParams, OperationType, Runner,
    RunnerId, RunnerOutcome,
};
use livstyle_profiles::Registry;

pub fn toolchain(profiles: &Path) -> Result<(), String> {
    let tc = Toolchain::detect(None);
    if tc.installs.is_empty() {
        println!("ESP-IDF: not found");
    }
    for (i, inst) in tc.installs.iter().enumerate() {
        let mark = if Some(i) == tc.selected { "*" } else { " " };
        println!(
            "{mark} ESP-IDF {} ({:?}) at {}\n    python: {}\n    activate: {}",
            inst.version,
            inst.source,
            inst.path.display(),
            inst.python
                .as_ref()
                .map_or("missing".into(), |p| p.display().to_string()),
            inst.activation_script
                .as_ref()
                .map_or("-".into(), |p| p.display().to_string()),
        );
    }
    let show = |name: &str, p: &Option<std::path::PathBuf>| {
        println!(
            "{name}: {}",
            p.as_ref()
                .map_or("not found".into(), |p| p.display().to_string())
        );
    };
    show("espflash", &tc.espflash);
    show("openocd", &tc.openocd);
    if let Ok(reg) = Registry::load_dir(profiles) {
        println!("\nchip support with the selected IDF:");
        for chip in reg.chips() {
            match tc.supports(chip) {
                Ok(_) => println!("  ok   {}", chip.chip),
                Err(e) => println!("  no   {}: {e}", chip.chip),
            }
        }
    }
    for p in &tc.problems {
        println!("warning: {p}");
    }
    Ok(())
}

pub fn devices(profiles: &Path) -> Result<(), String> {
    let devs = enumerate();
    if devs.is_empty() {
        println!("no USB serial devices (ttyACM*/ttyUSB*) found");
        println!("hint: check the cable is a data cable, and that you are in the `dialout` group");
        return Ok(());
    }
    let reg = Registry::load_dir(profiles).ok();
    for d in &devs {
        println!(
            "{}  {}  {:04x}:{:04x}  {:?}  {}",
            d.port.display(),
            d.id,
            d.vid,
            d.pid,
            d.bridge,
            d.product.as_deref().unwrap_or("")
        );
        if let Some(reg) = &reg {
            let key = d.bridge.profile_key();
            let candidates: Vec<&str> = reg
                .boards()
                .filter(|b| {
                    b.usb_bridge.as_deref() == Some(key)
                        || reg
                            .chip(&b.chip)
                            .is_some_and(|c| c.usb.native_ids.contains(&[d.vid, d.pid]))
                })
                .map(|b| b.id.as_str())
                .collect();
            if !candidates.is_empty() {
                println!(
                    "    possible boards: {} (run `livstyle chip-info` to identify)",
                    candidates.join(", ")
                );
            }
        }
    }
    Ok(())
}

pub fn chip_info(port_or_id: &str, yes: bool) -> Result<(), String> {
    let devs = enumerate();
    let dev = devs
        .iter()
        .find(|d| d.id == port_or_id || d.port.to_string_lossy() == port_or_id)
        .ok_or_else(|| format!("no device {port_or_id:?}; see `livstyle devices`"))?;
    if !yes {
        print!(
            "Reading chip info resets {} into download mode and back. The running firmware restarts. Continue? [y/N] ",
            dev.port.display()
        );
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            return Err("aborted".into());
        }
    }
    let tc = Toolchain::detect(None);
    let install = tc
        .selected()
        .ok_or("no ESP-IDF found (needed for esptool)")?;
    let mut runner = EspRunner::for_install("local", install, Box::new(SystemExec));
    let device_id = DeviceId::new(dev.id.clone());
    runner.set_ports(PortMap::from([(device_id.clone(), dev.port.clone())]));
    let params = OperationParams {
        operation_type: OperationType::ReadChipInfo,
        device_id,
        runner_id: RunnerId::new("local"),
        target: None,
        board_profile_id: None,
        artifact_sha256: None,
        regions: Vec::new(),
        extra: BTreeMap::new(),
    };
    let req = DispatchRequest {
        operation_id: OperationId::new("cli-chip-info"),
        task_id: TaskId::new("cli"),
        fencing_token: FencingToken(1),
        parameters_digest: params.digest(),
        params,
    };
    match runner.execute(&req) {
        RunnerOutcome::Completed(r) => {
            let chip = r.detected_chip.expect("chip-id parsed");
            println!(
                "{}: {} revision {}",
                dev.port.display(),
                chip.chip,
                chip.revision
            );
            Ok(())
        }
        RunnerOutcome::Failed(r) => Err(format!("{}\n{}", r.command_summary, r.output_summary)),
        other => Err(format!("{other:?}")),
    }
}
