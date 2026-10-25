//! USB serial discovery from sysfs. Ports are never opened here: opening a
//! port can toggle DTR/RTS and reset the board (an R2 effect).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbBridge {
    /// Espressif USB-Serial/JTAG (303a:1001): C3, C5, C6, S3, P4, H2 …
    EspUsbSerialJtag,
    /// Espressif native USB OTG running TinyUSB CDC or ROM download mode.
    EspNativeUsb,
    Cp210x,
    Ch340,
    Ch343,
    Ftdi,
    Other,
}

impl UsbBridge {
    pub fn classify(vid: u16, pid: u16) -> Self {
        match (vid, pid) {
            (0x303a, 0x1001) => Self::EspUsbSerialJtag,
            (0x303a, _) => Self::EspNativeUsb,
            (0x10c4, 0xea60) => Self::Cp210x,
            (0x1a86, 0x7523) => Self::Ch340,
            (0x1a86, 0x55d3 | 0x55d4) => Self::Ch343,
            (0x0403, 0x6001 | 0x6010 | 0x6014 | 0x6015) => Self::Ftdi,
            _ => Self::Other,
        }
    }

    /// Profile key used in `BoardProfile::usb_bridge`.
    pub fn profile_key(self) -> &'static str {
        match self {
            Self::EspUsbSerialJtag => "usb-serial-jtag",
            Self::EspNativeUsb => "native-usb",
            Self::Cp210x => "cp210x",
            Self::Ch340 => "ch340",
            Self::Ch343 => "ch343",
            Self::Ftdi => "ftdi",
            Self::Other => "other",
        }
    }

    pub fn likely_espressif_target(self) -> bool {
        !matches!(self, Self::Other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SerialDevice {
    /// Stable id: USB serial number when present, otherwise the physical port path.
    pub id: String,
    pub port: PathBuf,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// USB topology path, e.g. `1-4.2`; stable across replug in the same socket.
    pub usb_path: Option<String>,
    pub bridge: UsbBridge,
}

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn hex16(p: &Path) -> Option<u16> {
    u16::from_str_radix(&read_trim(p)?, 16).ok()
}

/// Enumerates `ttyACM*` / `ttyUSB*` under `sys_root` (normally `/sys`) and
/// maps them to `/dev`.
pub fn enumerate_at(sys_root: &Path, dev_root: &Path) -> Vec<SerialDevice> {
    let Ok(entries) = fs::read_dir(sys_root.join("class/tty")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("ttyACM") || name.starts_with("ttyUSB")) {
            continue;
        }
        let Ok(mut dir) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        // Walk up from the interface to the USB device node holding idVendor.
        let usb_dev = loop {
            if dir.join("idVendor").is_file() {
                break Some(dir.clone());
            }
            if !dir.pop() || dir == sys_root {
                break None;
            }
        };
        let Some(usb) = usb_dev else { continue };
        let (Some(vid), Some(pid)) = (hex16(&usb.join("idVendor")), hex16(&usb.join("idProduct")))
        else {
            continue;
        };
        let serial = read_trim(&usb.join("serial"));
        let usb_path = usb.file_name().map(|n| n.to_string_lossy().into_owned());
        let id = match (&serial, &usb_path) {
            (Some(s), _) => format!("usb-{vid:04x}-{pid:04x}-{s}"),
            (None, Some(p)) => format!("usbpath-{p}"),
            (None, None) => format!("tty-{name}"),
        };
        out.push(SerialDevice {
            id,
            port: dev_root.join(&name),
            vid,
            pid,
            serial,
            manufacturer: read_trim(&usb.join("manufacturer")),
            product: read_trim(&usb.join("product")),
            usb_path,
            bridge: UsbBridge::classify(vid, pid),
        });
    }
    out.sort_by(|a, b| a.port.cmp(&b.port));
    out
}

pub fn enumerate() -> Vec<SerialDevice> {
    // Linux reads sysfs and does not open the port. Windows SetupAPI and
    // Android USB are not implemented; those targets get an empty list
    // rather than a guessed device.
    #[cfg(target_os = "linux")]
    {
        enumerate_at(Path::new("/sys"), Path::new("/dev"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_device(sys: &Path, tty: &str, usb: &str, vid: &str, pid: &str, serial: Option<&str>) {
        let usb_dir: PathBuf = sys.join("devices/pci0000:00/usb1").join(usb);
        let iface = usb_dir.join(format!("{usb}:1.0"));
        fs::create_dir_all(iface.join("tty").join(tty)).unwrap();
        fs::write(usb_dir.join("idVendor"), vid).unwrap();
        fs::write(usb_dir.join("idProduct"), pid).unwrap();
        fs::write(usb_dir.join("product"), "USB JTAG/serial debug unit\n").unwrap();
        if let Some(s) = serial {
            fs::write(usb_dir.join("serial"), s).unwrap();
        }
        let class = sys.join("class/tty").join(tty);
        fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink(&iface, class.join("device")).unwrap();
    }

    #[test]
    fn enumerates_and_classifies() {
        let root = std::env::temp_dir().join(format!("livstyle-sysfs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sys = root.join("sys");
        fake_device(
            &sys,
            "ttyACM0",
            "1-4",
            "303a",
            "1001",
            Some("F0:F5:BD:01:02:03"),
        );
        fake_device(&sys, "ttyUSB0", "1-5.1", "10c4", "ea60", None);
        fs::create_dir_all(sys.join("class/tty/ttyS0")).unwrap();

        let devs = enumerate_at(&fs::canonicalize(&sys).unwrap(), Path::new("/dev"));
        assert_eq!(devs.len(), 2, "{devs:?}");
        assert_eq!(devs[0].port, PathBuf::from("/dev/ttyACM0"));
        assert_eq!(devs[0].bridge, UsbBridge::EspUsbSerialJtag);
        assert_eq!(devs[0].id, "usb-303a-1001-F0:F5:BD:01:02:03");
        assert_eq!(devs[1].bridge, UsbBridge::Cp210x);
        assert_eq!(devs[1].id, "usbpath-1-5.1");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn classify_table() {
        assert_eq!(UsbBridge::classify(0x303a, 0x0002), UsbBridge::EspNativeUsb);
        assert_eq!(UsbBridge::classify(0x1a86, 0x55d3), UsbBridge::Ch343);
        assert!(!UsbBridge::classify(0x046d, 0xc52b).likely_espressif_target());
    }
}
