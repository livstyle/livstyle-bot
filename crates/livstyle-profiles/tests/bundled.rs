use std::fs;
use std::path::PathBuf;

use livstyle_profiles::{Arch, Capability, ChipRevision, ProfileError, Registry, SupportTier};

fn bundled() -> Registry {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
    Registry::load_dir(root).expect("bundled profiles must load")
}

#[test]
fn first_batch_chips_are_present() {
    let registry = bundled();
    for chip in [
        "esp32", "esp32s3", "esp32c3", "esp32c5", "esp32c6", "esp32p4",
    ] {
        assert!(registry.chip(chip).is_some(), "missing chip profile {chip}");
    }
    assert_eq!(registry.chip("esp32s3").unwrap().arch, Arch::Xtensa);
    assert_eq!(registry.chip("esp32c6").unwrap().arch, Arch::Riscv);
}

#[test]
fn acceptance_set_covers_both_architectures() {
    let registry = bundled();
    let verified: Vec<_> = registry
        .boards()
        .filter(|b| registry.effective_tier(&b.id) == SupportTier::Verified)
        .map(|b| registry.chip(&b.chip).unwrap().arch)
        .collect();
    assert!(verified.contains(&Arch::Xtensa));
    assert!(verified.contains(&Arch::Riscv));
}

/// ADR-007: S3 (Xtensa) and C5 (RISC-V) are the required acceptance boards;
/// C3 and C6 boards are on hand for regression.
#[test]
fn acceptance_boards_are_s3_and_c5() {
    let registry = bundled();
    for board in [
        "esp32s3-devkitc-1-n8r8",
        "esp32c5-devkitc-1",
        "esp32c3-devkitm-1",
        "esp32c6-devkitc-1-n8",
    ] {
        assert_eq!(
            registry.effective_tier(board),
            SupportTier::Verified,
            "{board}"
        );
    }
    let c5 = registry.board("esp32c5-devkitc-1").unwrap();
    assert!(c5.is_reserved(28).is_some(), "boot strapping pin");
    assert!(
        registry
            .provider_of("esp32c5-devkitc-1", Capability::Wifi5Ghz)
            .is_some()
    );
}

#[test]
fn planning_constraints_follow_chip_capabilities() {
    let registry = bundled();

    let c3 = registry.chip("esp32c3").unwrap();
    assert!(c3.has(Capability::Wifi));
    assert!(!c3.has(Capability::Ieee802154));
    assert!(registry.chip("esp32c5").unwrap().has(Capability::Wifi5Ghz));

    let p4 = registry.chip("esp32p4").unwrap();
    assert!(!p4.has(Capability::Wifi));
    let provider = registry
        .provider_of("esp32p4-function-ev-board", Capability::Wifi)
        .expect("P4 board gets Wi-Fi from its companion");
    assert_eq!(provider.chip, "esp32c6");
}

#[test]
fn experimental_chip_caps_board_tier() {
    let registry = bundled();
    assert_eq!(
        registry.effective_tier("esp32p4-function-ev-board"),
        SupportTier::Experimental
    );
    assert_eq!(
        registry.effective_tier("no-such-board"),
        SupportTier::Unsupported
    );
}

#[test]
fn reserved_pins_are_queryable() {
    let registry = bundled();
    let board = registry.board("esp32c3-devkitm-1").unwrap();
    assert!(board.is_reserved(9).is_some());
    assert!(board.is_reserved(4).is_none());
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("livstyle-profiles-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("chips")).unwrap();
    fs::create_dir_all(dir.join("boards")).unwrap();
    dir
}

#[test]
fn new_chip_needs_only_a_profile() {
    let dir = scratch_dir("h2");
    fs::write(
        dir.join("chips/esp32h2.toml"),
        r#"
schema_version = 1
chip = "esp32h2"
display_name = "ESP32-H2"
arch = "riscv"
hp_cores = 1
support_tier = "experimental"
idf = { min_version = "5.1" }
revisions = { min = "v0.0" }
usb = { usb_serial_jtag = true }
debug = { openocd_target = "target/esp32h2.cfg", openocd_interface = "interface/esp_usb_jtag.cfg", gdb = "riscv32-esp-elf-gdb" }
radio = { ble = true, ieee802154 = true }
provenance = { status = "draft" }
"#,
    )
    .unwrap();

    let registry = Registry::load_dir(&dir).unwrap();
    let h2 = registry.chip("esp32h2").unwrap();
    assert!(h2.has(Capability::Ieee802154));
    assert!(!h2.has(Capability::Wifi));
    assert!(h2.revisions.allows(ChipRevision::new(1, 2)));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn board_with_unknown_chip_is_rejected() {
    let dir = scratch_dir("orphan");
    fs::write(
        dir.join("boards/orphan.toml"),
        r#"
schema_version = 1
id = "orphan"
display_name = "Orphan"
chip = "esp32x9"
support_tier = "experimental"
flash_size_mb = 4
console = "usb_serial_jtag"
auto_download = true
flash = { mode = "dio", freq = "40m", partition_table = "partitions_singleapp" }
provenance = { status = "draft" }
"#,
    )
    .unwrap();

    let err = Registry::load_dir(&dir).unwrap_err();
    assert!(matches!(err, ProfileError::UnknownChip { .. }), "{err}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unknown_fields_are_rejected() {
    let dir = scratch_dir("typo");
    let s3 = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/chips/esp32s3.toml"),
    )
    .unwrap();
    fs::write(
        dir.join("chips/esp32s3.toml"),
        s3.replace("hp_cores", "hp_core"),
    )
    .unwrap();
    assert!(matches!(
        Registry::load_dir(&dir).unwrap_err(),
        ProfileError::Parse { .. }
    ));
    fs::remove_dir_all(dir).unwrap();
}
