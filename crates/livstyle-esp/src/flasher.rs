//! `build/flasher_args.json` produced by ESP-IDF: the authoritative list of
//! images, offsets and write-flash options for a build.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashFile {
    pub offset: u32,
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlasherArgs {
    pub chip: String,
    pub write_flash_args: Vec<String>,
    /// Sorted by offset.
    pub files: Vec<FlashFile>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    write_flash_args: Vec<String>,
    flash_files: BTreeMap<String, String>,
    #[serde(default)]
    extra_esptool_args: BTreeMap<String, serde_json::Value>,
}

fn parse_offset(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

impl FlasherArgs {
    pub fn load(build_dir: &Path) -> Result<Self, String> {
        let path = build_dir.join("flasher_args.json");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let raw: Raw =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let chip = raw
            .extra_esptool_args
            .get("chip")
            .and_then(|v| v.as_str())
            .ok_or("flasher_args.json has no extra_esptool_args.chip")?
            .to_owned();
        let mut files = Vec::new();
        for (off, rel) in raw.flash_files {
            let offset = parse_offset(&off).ok_or_else(|| format!("bad offset {off:?}"))?;
            let file = build_dir.join(&rel);
            let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            files.push(FlashFile {
                offset,
                path: file,
                sha256: hex::encode(Sha256::digest(&bytes)),
            });
        }
        files.sort_by_key(|f| f.offset);
        Ok(Self {
            chip,
            write_flash_args: raw.write_flash_args,
            files,
        })
    }

    /// One digest over all `(offset, sha256)` pairs: the artifact identity an
    /// approval binds to.
    pub fn artifact_sha256(&self) -> String {
        let mut h = Sha256::new();
        for f in &self.files {
            h.update(f.offset.to_be_bytes());
            h.update(f.sha256.as_bytes());
        }
        hex::encode(h.finalize())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fake_build(dir: &Path, chip: &str) {
        std::fs::create_dir_all(dir.join("bootloader")).unwrap();
        std::fs::create_dir_all(dir.join("partition_table")).unwrap();
        std::fs::write(dir.join("bootloader/bootloader.bin"), b"boot").unwrap();
        std::fs::write(dir.join("partition_table/partition-table.bin"), b"pt").unwrap();
        std::fs::write(dir.join("app.bin"), b"app").unwrap();
        std::fs::write(
            dir.join("flasher_args.json"),
            serde_json::json!({
                "write_flash_args": ["--flash-mode", "dio", "--flash-freq", "80m", "--flash-size", "8MB"],
                "flash_settings": {},
                "flash_files": {
                    "0x10000": "app.bin",
                    "0x2000": "bootloader/bootloader.bin",
                    "0x8000": "partition_table/partition-table.bin"
                },
                "extra_esptool_args": { "after": "hard-reset", "before": "default-reset", "stub": true, "chip": chip }
            })
            .to_string(),
        )
        .unwrap();
    }

    /// Set `LIVSTYLE_TEST_IDF_BUILD=<project>/build` to check a real IDF build.
    #[test]
    fn real_idf_build_if_provided() {
        let Some(dir) = std::env::var_os("LIVSTYLE_TEST_IDF_BUILD") else {
            return;
        };
        let fa = FlasherArgs::load(Path::new(&dir)).unwrap();
        assert!(fa.chip.starts_with("esp32"));
        assert!(fa.files.len() >= 3, "{:?}", fa.files);
        assert!(fa.write_flash_args.iter().any(|a| a.contains("flash")));
        eprintln!(
            "{} files for {}, artifact {}",
            fa.files.len(),
            fa.chip,
            fa.artifact_sha256()
        );
    }

    #[test]
    fn loads_and_hashes() {
        let dir = std::env::temp_dir().join(format!("livstyle-flasher-{}", std::process::id()));
        fake_build(&dir, "esp32c5");
        let fa = FlasherArgs::load(&dir).unwrap();
        assert_eq!(fa.chip, "esp32c5");
        let offs: Vec<_> = fa.files.iter().map(|f| f.offset).collect();
        assert_eq!(offs, [0x2000, 0x8000, 0x10000]);
        let before = fa.artifact_sha256();
        std::fs::write(dir.join("app.bin"), b"app2").unwrap();
        assert_ne!(FlasherArgs::load(&dir).unwrap().artifact_sha256(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
