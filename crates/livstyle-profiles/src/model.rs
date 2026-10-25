use serde::{Deserialize, Serialize};

use crate::ChipRevision;

/// Support tier (PRD §11.2). Ordered from weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    Unsupported,
    Experimental,
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    Xtensa,
    Riscv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceStatus {
    /// Values copied from secondary sources; must be checked against
    /// Espressif datasheets before the profile can be marked verified.
    Draft,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub status: ProvenanceStatus,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdfSupport {
    /// Minimum ESP-IDF version, e.g. `5.1`.
    pub min_version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionPolicy {
    pub min: Option<ChipRevision>,
    pub max: Option<ChipRevision>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl RevisionPolicy {
    pub fn allows(&self, revision: ChipRevision) -> bool {
        self.min.is_none_or(|min| revision >= min) && self.max.is_none_or(|max| revision <= max)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsbInfo {
    /// Built-in USB-Serial-JTAG peripheral.
    pub usb_serial_jtag: bool,
    /// Native USB `(vid, pid)` pairs. A shared PID (e.g. 0x1001) only narrows
    /// the candidates; the chip must still be confirmed with esptool.
    #[serde(default)]
    pub native_ids: Vec<[u16; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugInfo {
    pub openocd_target: String,
    pub openocd_interface: String,
    pub gdb: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiBand {
    #[serde(rename = "2.4ghz")]
    Ghz2_4,
    #[serde(rename = "5ghz")]
    Ghz5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WifiSpec {
    /// e.g. `wifi4`, `wifi6`.
    pub standard: String,
    pub bands: Vec<WifiBand>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Radio {
    pub wifi: Option<WifiSpec>,
    #[serde(default)]
    pub bt_classic: bool,
    #[serde(default)]
    pub ble: bool,
    #[serde(default)]
    pub ieee802154: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootInfo {
    /// Substrings expected in a healthy boot log, used as default assertions.
    #[serde(default)]
    pub log_patterns: Vec<String>,
}

/// Capability an agent plan may require (PRD §11.2 planning constraints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Wifi,
    Wifi5Ghz,
    Wifi6,
    BtClassic,
    Ble,
    Ieee802154,
    UsbSerialJtag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipProfile {
    pub schema_version: u32,
    /// esptool / `idf_target` name, e.g. `esp32c6`.
    pub chip: String,
    pub display_name: String,
    pub arch: Arch,
    pub hp_cores: u8,
    #[serde(default)]
    pub lp_core: bool,
    pub support_tier: SupportTier,
    pub idf: IdfSupport,
    #[serde(default)]
    pub revisions: RevisionPolicy,
    pub usb: UsbInfo,
    pub debug: DebugInfo,
    #[serde(default)]
    pub radio: Radio,
    #[serde(default)]
    pub boot: BootInfo,
    pub provenance: Provenance,
}

impl ChipProfile {
    pub fn has(&self, capability: Capability) -> bool {
        let wifi = self.radio.wifi.as_ref();
        match capability {
            Capability::Wifi => wifi.is_some(),
            Capability::Wifi5Ghz => wifi.is_some_and(|w| w.bands.contains(&WifiBand::Ghz5)),
            Capability::Wifi6 => wifi.is_some_and(|w| w.standard == "wifi6"),
            Capability::BtClassic => self.radio.bt_classic,
            Capability::Ble => self.radio.ble,
            Capability::Ieee802154 => self.radio.ieee802154,
            Capability::UsbSerialJtag => self.usb.usb_serial_jtag,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Console {
    UartBridge,
    UsbSerialJtag,
    UsbCdc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Psram {
    /// e.g. `quad`, `octal`, `hex`.
    pub kind: String,
    pub size_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservedPin {
    pub gpio: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardPeripheral {
    pub name: String,
    pub gpio: Option<u8>,
}

/// Companion chip that provides capabilities the main chip lacks,
/// e.g. an ESP32-C6 providing Wi-Fi for an ESP32-P4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Companion {
    pub chip: String,
    pub role: String,
    /// Host link, e.g. `esp-hosted-sdio`.
    pub link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashDefaults {
    pub mode: String,
    pub freq: String,
    pub partition_table: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardProfile {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub chip: String,
    pub module: Option<String>,
    pub support_tier: SupportTier,
    pub flash_size_mb: u32,
    pub psram: Option<Psram>,
    pub console: Console,
    /// USB-UART bridge chip, e.g. `cp210x`, `ch340`.
    pub usb_bridge: Option<String>,
    /// Whether the auto-download circuit lets esptool enter download mode
    /// without pressing BOOT.
    pub auto_download: bool,
    #[serde(default)]
    pub reserved_pins: Vec<ReservedPin>,
    #[serde(default)]
    pub onboard: Vec<OnboardPeripheral>,
    pub companion: Option<Companion>,
    pub flash: FlashDefaults,
    pub provenance: Provenance,
}

impl BoardProfile {
    pub fn is_reserved(&self, gpio: u8) -> Option<&ReservedPin> {
        self.reserved_pins.iter().find(|p| p.gpio == gpio)
    }
}
