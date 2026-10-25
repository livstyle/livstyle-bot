use livstyle_hardware::DetectedChip;
use livstyle_profiles::ChipRevision;

/// `ESP32-C5`, `esp32c5`, `ESP32-S3 (QFN56)` → `esp32c5` / `esp32s3`.
pub fn normalize_chip_name(raw: &str) -> String {
    let head = raw.split(['(', ' ']).next().unwrap_or(raw);
    head.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Parses chip and revision from esptool (v4 `Chip is …`, v5 `Chip type: …`)
/// or espflash `board-info` output.
pub fn parse_chip_info(output: &str) -> Option<DetectedChip> {
    for line in output.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("Chip is ")
            .or_else(|| line.strip_prefix("Chip type:"))
            .or_else(|| line.strip_prefix("Detected chip:"));
        let Some(rest) = rest else { continue };
        let rest = rest.trim();
        let chip = normalize_chip_name(rest);
        if !chip.starts_with("esp32") {
            continue;
        }
        let revision = rest
            .find("revision")
            .and_then(|i| rest[i + "revision".len()..].split([')', ',']).next())
            .and_then(|r| r.trim().parse::<ChipRevision>().ok())?;
        return Some(DetectedChip { chip, revision });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esptool_v5() {
        let out = "esptool v5.1.0\nConnecting....\nConnected to ESP32-C5 on /dev/ttyACM0:\nChip type:          ESP32-C5 (QFN32) (revision v0.1)\nFeatures:           Wi-Fi 6 (dual-band), BT 5 (LE), IEEE802.15.4\n";
        let c = parse_chip_info(out).unwrap();
        assert_eq!(c.chip, "esp32c5");
        assert_eq!(c.revision, ChipRevision::new(0, 1));
    }

    #[test]
    fn esptool_v4_and_espflash() {
        let v4 = "Chip is ESP32-S3 (QFN56) (revision v0.2)\n";
        assert_eq!(parse_chip_info(v4).unwrap().chip, "esp32s3");
        let ef = "[INFO] Serial port: '/dev/ttyACM0'\nChip type:         esp32c3 (revision v0.4)\nCrystal frequency: 40 MHz\n";
        let c = parse_chip_info(ef).unwrap();
        assert_eq!(
            (c.chip.as_str(), c.revision),
            ("esp32c3", ChipRevision::new(0, 4))
        );
        assert!(parse_chip_info("A fatal error occurred: Failed to connect").is_none());
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_chip_name("ESP32-C6"), "esp32c6");
        assert_eq!(normalize_chip_name("ESP32 (revision v3.1)"), "esp32");
    }
}
