//! ESP32-family integration (PRD §11, ADR-013).
//!
//! * [`toolchain`]: find ESP-IDF (EIM first, then `IDF_PATH`) and check it
//!   against a chip profile's minimum IDF version.
//! * [`devices`]: enumerate USB serial ports from sysfs without opening them,
//!   so discovery never resets a board.
//! * [`EspRunner`]: the [`livstyle_hardware::Runner`] that drives esptool
//!   from the selected IDF's Python environment, with fencing, ChipCheck and
//!   artifact hash verification before every write.

mod chipinfo;
mod command;
pub mod devices;
mod flasher;
mod job;
mod runner;
pub mod serial;
pub mod toolchain;

pub use chipinfo::{normalize_chip_name, parse_chip_info};
pub use command::{CommandOutput, Exec, SystemExec};
pub use devices::{SerialDevice, UsbBridge, enumerate};
pub use flasher::{FlashFile, FlasherArgs};
pub use job::{boot_log_acceptable, commit_flash, flash_offer, write_evidence, FlashOffer};
pub use runner::{EspRunner, PortMap};
#[cfg(unix)]
pub use serial::read_log;

/// Identifies the chip on `port` with the selected ESP-IDF esptool.
/// Resets the board. Discovery ([`enumerate`]) does not.
pub fn identify_chip(port: &std::path::Path) -> Result<String, String> {
    let tc = Toolchain::detect(None);
    let install = tc
        .selected()
        .ok_or("没有找到 ESP-IDF，识别芯片需要它自带的 esptool")?;
    let runner = EspRunner::for_install("ui", install, Box::new(SystemExec));
    runner.read_chip_line(port)
}
pub use toolchain::{IdfInstall, IdfVersion, Toolchain};
