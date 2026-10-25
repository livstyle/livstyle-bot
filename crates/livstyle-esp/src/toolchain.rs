use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use livstyle_profiles::ChipProfile;
use serde::Deserialize;

/// ESP-IDF `major.minor.patch`. Profiles state minimums as `5.5`, meaning `5.5.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdfVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl IdfVersion {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let core = s.split(['-', ' ']).next()?;
        let mut parts = core.split('.').map(|p| p.parse::<u32>());
        let major = parts.next()?.ok()?;
        let minor = parts.next().unwrap_or(Ok(0)).ok()?;
        let patch = parts.next().unwrap_or(Ok(0)).ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Reads `tools/cmake/version.cmake`, the source of truth inside an IDF tree.
    pub fn from_idf_tree(idf_path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(idf_path.join("tools/cmake/version.cmake")).ok()?;
        let get = |key: &str| -> Option<u32> {
            text.lines().find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("set({key} "))?
                    .strip_suffix(')')?
                    .trim()
                    .parse()
                    .ok()
            })
        };
        Some(Self {
            major: get("IDF_VERSION_MAJOR")?,
            minor: get("IDF_VERSION_MINOR")?,
            patch: get("IDF_VERSION_PATCH")?,
        })
    }
}

impl fmt::Display for IdfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// Espressif Installation Manager (`~/.espressif/tools/eim_idf.json`).
    Eim,
    /// `IDF_PATH` from the environment.
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdfInstall {
    pub source: InstallSource,
    pub name: String,
    pub path: PathBuf,
    pub version: IdfVersion,
    pub python: Option<PathBuf>,
    pub activation_script: Option<PathBuf>,
    pub tools_path: Option<PathBuf>,
}

impl IdfInstall {
    /// `idf.py <args>` inside the activated environment. Uses the EIM
    /// activation script when present so PATH and IDF_PYTHON_ENV_PATH match
    /// what the user's own shell gets.
    pub fn idf_py(&self, args: &[&str]) -> Command {
        match &self.activation_script {
            Some(script) => {
                let mut c = Command::new("bash");
                c.arg("-c")
                    .arg(r#". "$1" >/dev/null 2>&1 || exit 97; shift; exec idf.py "$@""#)
                    .arg("livstyle")
                    .arg(script)
                    .args(args);
                c
            }
            None => {
                let mut c = Command::new(self.python.clone().unwrap_or_else(|| "python3".into()));
                c.arg(self.path.join("tools/idf.py"))
                    .args(args)
                    .env("IDF_PATH", &self.path);
                c
            }
        }
    }

    /// `python -m esptool <args>` from the IDF Python environment, so the
    /// esptool version matches the IDF that produced `flasher_args.json`.
    pub fn esptool(&self) -> (PathBuf, Vec<String>) {
        let python = self.python.clone().unwrap_or_else(|| "python3".into());
        (python, vec!["-m".into(), "esptool".into()])
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EimFile {
    #[serde(default)]
    idf_installed: Vec<EimEntry>,
    #[serde(default)]
    idf_selected_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EimEntry {
    id: String,
    name: String,
    path: PathBuf,
    #[serde(default)]
    python: Option<PathBuf>,
    #[serde(default)]
    activation_script: Option<PathBuf>,
    #[serde(default)]
    idf_tools_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct Toolchain {
    /// All usable installs; `selected` indexes into it.
    pub installs: Vec<IdfInstall>,
    pub selected: Option<usize>,
    pub espflash: Option<PathBuf>,
    pub openocd: Option<PathBuf>,
    pub problems: Vec<String>,
}

fn which(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    extra_dirs
        .iter()
        .cloned()
        .chain(std::env::split_paths(&path))
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

impl Toolchain {
    /// Detects installs. `eim_json` defaults to `~/.espressif/tools/eim_idf.json`.
    pub fn detect(eim_json: Option<&Path>) -> Self {
        let mut tc = Self::default();
        let default_eim = home().map(|h| h.join(".espressif/tools/eim_idf.json"));
        let eim_path = eim_json.map(Path::to_path_buf).or(default_eim);
        if let Some(path) = eim_path.filter(|p| p.is_file()) {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<EimFile>(&t).map_err(|e| e.to_string()))
            {
                Ok(eim) => {
                    for entry in eim.idf_installed {
                        let Some(version) = IdfVersion::from_idf_tree(&entry.path) else {
                            tc.problems.push(format!(
                                "EIM install {:?} at {} has no readable version.cmake",
                                entry.name,
                                entry.path.display()
                            ));
                            continue;
                        };
                        if eim.idf_selected_id.as_deref() == Some(entry.id.as_str()) {
                            tc.selected = Some(tc.installs.len());
                        }
                        tc.installs.push(IdfInstall {
                            source: InstallSource::Eim,
                            name: entry.name,
                            path: entry.path,
                            version,
                            python: entry.python.filter(|p| p.is_file()),
                            activation_script: entry.activation_script.filter(|p| p.is_file()),
                            tools_path: entry.idf_tools_path,
                        });
                    }
                }
                Err(e) => tc
                    .problems
                    .push(format!("cannot read {}: {e}", path.display())),
            }
        }
        if let Some(idf_path) = std::env::var_os("IDF_PATH").map(PathBuf::from) {
            let known = tc.installs.iter().any(|i| i.path == idf_path);
            match IdfVersion::from_idf_tree(&idf_path) {
                Some(version) if !known => {
                    let python = std::env::var_os("IDF_PYTHON_ENV_PATH")
                        .map(|p| PathBuf::from(p).join("bin/python"))
                        .filter(|p| p.is_file());
                    tc.installs.push(IdfInstall {
                        source: InstallSource::Env,
                        name: format!("v{version}"),
                        path: idf_path,
                        version,
                        python,
                        activation_script: None,
                        tools_path: std::env::var_os("IDF_TOOLS_PATH").map(PathBuf::from),
                    });
                    // An explicitly exported IDF_PATH wins over the EIM selection.
                    tc.selected = Some(tc.installs.len() - 1);
                }
                Some(_) => {}
                None => tc.problems.push(format!(
                    "IDF_PATH={} is not an ESP-IDF tree",
                    idf_path.display()
                )),
            }
        }
        if tc.selected.is_none() && !tc.installs.is_empty() {
            tc.selected = Some(0);
        }
        if tc.installs.is_empty() {
            tc.problems
                .push("no ESP-IDF found: install with EIM (https://dl.espressif.com/dl/eim/) or export IDF_PATH".into());
        }
        for install in &tc.installs {
            if install.python.is_none() {
                tc.problems
                    .push(format!("IDF {} has no Python environment", install.name));
            }
        }

        let cargo_bin: Vec<PathBuf> = home().map(|h| h.join(".cargo/bin")).into_iter().collect();
        tc.espflash = which("espflash", &cargo_bin);
        let openocd_dirs: Vec<PathBuf> = tc
            .selected()
            .and_then(|i| i.tools_path.clone())
            .map(|t| find_openocd(&t))
            .unwrap_or_default();
        tc.openocd = which("openocd", &openocd_dirs);
        tc
    }

    pub fn selected(&self) -> Option<&IdfInstall> {
        self.selected.and_then(|i| self.installs.get(i))
    }

    /// The selected IDF must be at least the chip profile's minimum.
    pub fn supports(&self, chip: &ChipProfile) -> Result<&IdfInstall, String> {
        let install = self.selected().ok_or("no ESP-IDF selected")?;
        let min = IdfVersion::parse(&chip.idf.min_version)
            .ok_or_else(|| format!("chip {} has an invalid idf.min_version", chip.chip))?;
        match install.version.cmp(&min) {
            Ordering::Less => Err(format!(
                "{} needs ESP-IDF >= {min}, selected {} is {}",
                chip.display_name, install.name, install.version
            )),
            _ => Ok(install),
        }
    }
}

fn find_openocd(tools: &Path) -> Vec<PathBuf> {
    let base = tools.join("openocd-esp32");
    let Ok(versions) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = versions
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("openocd-esp32/bin"))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(
            IdfVersion::parse("5.5"),
            Some(IdfVersion {
                major: 5,
                minor: 5,
                patch: 0
            })
        );
        assert_eq!(
            IdfVersion::parse("v6.0.2"),
            Some(IdfVersion {
                major: 6,
                minor: 0,
                patch: 2
            })
        );
        assert_eq!(
            IdfVersion::parse("v5.4-dev-123"),
            Some(IdfVersion {
                major: 5,
                minor: 4,
                patch: 0
            })
        );
        assert!(IdfVersion::parse("main").is_none());
        assert!(IdfVersion::parse("6.0.2").unwrap() > IdfVersion::parse("5.5").unwrap());
    }

    #[test]
    fn reads_eim_file_and_version_cmake() {
        let dir = std::env::temp_dir().join(format!("livstyle-eim-{}", std::process::id()));
        let idf = dir.join("esp-idf");
        std::fs::create_dir_all(idf.join("tools/cmake")).unwrap();
        std::fs::write(
            idf.join("tools/cmake/version.cmake"),
            "set(IDF_VERSION_MAJOR 5)\nset(IDF_VERSION_MINOR 4)\nset(IDF_VERSION_PATCH 1)\n",
        )
        .unwrap();
        let eim = dir.join("eim_idf.json");
        std::fs::write(
            &eim,
            serde_json::json!({
                "idfInstalled": [{ "id": "a", "name": "v5.4.1", "path": idf, "python": "/nonexistent" }],
                "idfSelectedId": "a",
                "version": "2.0"
            })
            .to_string(),
        )
        .unwrap();
        // Only meaningful without an exported IDF_PATH.
        if std::env::var_os("IDF_PATH").is_none() {
            let tc = Toolchain::detect(Some(&eim));
            let sel = tc.selected().unwrap();
            assert_eq!(sel.version.to_string(), "5.4.1");
            assert_eq!(sel.source, InstallSource::Eim);
            assert!(tc.problems.iter().any(|p| p.contains("no Python")));

            let c5: ChipProfile = toml_chip("esp32c5", "5.5");
            assert!(tc.supports(&c5).unwrap_err().contains(">= 5.5.0"));
            let c3 = toml_chip("esp32c3", "4.3");
            assert!(tc.supports(&c3).is_ok());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn toml_chip(chip: &str, min: &str) -> ChipProfile {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let reg = livstyle_profiles::Registry::load_dir(root).unwrap();
        let mut c = reg.chip(chip).unwrap().clone();
        c.idf.min_version = min.into();
        c
    }
}
