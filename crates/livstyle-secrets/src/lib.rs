//! Credential storage (PRD §9.2).
//!
//! Secrets live in `$XDG_CONFIG_HOME/livstyle/credentials.toml` (mode 0600),
//! never in project files, logs or agent context. An OS keyring backend can
//! replace the file later behind the same API (ADR-003).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURSOR_API_KEY: &str = "cursor.api_key";

/// A secret string whose `Debug`/`Display` never reveal the value and whose
/// buffer is overwritten on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Display form like `key_ab…yz` for confirming which key is stored.
    pub fn fingerprint(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        if chars.len() <= 8 {
            return "•".repeat(chars.len());
        }
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 2..].iter().collect();
        format!("{head}…{tail}")
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // SAFETY: zero bytes are valid UTF-8.
        unsafe { self.0.as_bytes_mut() }.fill(0);
    }
}

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("no config directory (set XDG_CONFIG_HOME or HOME)")]
    NoConfigDir,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{path} is readable by other users; run `chmod 600 {path}`")]
    InsecurePermissions { path: PathBuf },
    #[error("secret {0:?} must not be empty")]
    Empty(String),
}

#[derive(Default, Serialize, Deserialize)]
struct FileFormat {
    #[serde(default)]
    secrets: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SecretHit {
    pub secret: Secret,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_location() -> Result<Self, SecretError> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or(SecretError::NoConfigDir)?;
        Ok(Self::at(base.join("livstyle").join("credentials.toml")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Process environment, then a gitignored env file, then the credentials file.
    /// For the Cursor key the file may use `CURSOR_API_KEY=`.
    pub fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self.get_hit(name)?.map(|hit| hit.secret))
    }

    pub fn get_hit(&self, name: &str) -> Result<Option<SecretHit>, SecretError> {
        for key in env_keys(name) {
            if let Ok(v) = std::env::var(&key) {
                if !v.is_empty() {
                    return Ok(Some(SecretHit {
                        secret: Secret::new(v),
                        source: format!("environment {key}"),
                    }));
                }
            }
        }
        for (path, pairs) in read_env_files()? {
            for key in env_keys(name) {
                if let Some(v) = pairs.get(&key) {
                    if !v.is_empty() {
                        return Ok(Some(SecretHit {
                            secret: Secret::new(v.clone()),
                            source: path.display().to_string(),
                        }));
                    }
                }
            }
        }
        Ok(self.load()?.secrets.get(name).cloned().map(|v| SecretHit {
            secret: Secret::new(v),
            source: self.path.display().to_string(),
        }))
    }

    pub fn set(&self, name: &str, value: &Secret) -> Result<(), SecretError> {
        if value.expose().trim().is_empty() {
            return Err(SecretError::Empty(name.to_owned()));
        }
        let mut data = self.load()?;
        data.secrets
            .insert(name.to_owned(), value.expose().trim().to_owned());
        self.save(&data)
    }

    pub fn remove(&self, name: &str) -> Result<bool, SecretError> {
        let mut data = self.load()?;
        let removed = data.secrets.remove(name).is_some();
        if removed {
            self.save(&data)?;
        }
        Ok(removed)
    }

    fn io(&self, source: std::io::Error) -> SecretError {
        SecretError::Io {
            path: self.path.clone(),
            source,
        }
    }

    fn load(&self) -> Result<FileFormat, SecretError> {
        if !self.path.exists() {
            return Ok(FileFormat::default());
        }
        check_permissions(&self.path)?;
        let text = fs::read_to_string(&self.path).map_err(|e| self.io(e))?;
        toml::from_str(&text).map_err(|source| SecretError::Parse {
            path: self.path.clone(),
            source,
        })
    }

    fn save(&self, data: &FileFormat) -> Result<(), SecretError> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| self.io(e))?;
        }
        let text = toml::to_string(data).expect("credentials serialize");
        let tmp = self.path.with_extension("toml.tmp");
        let mut file = open_private(&tmp).map_err(|e| self.io(e))?;
        file.write_all(text.as_bytes()).map_err(|e| self.io(e))?;
        file.sync_all().map_err(|e| self.io(e))?;
        fs::rename(&tmp, &self.path).map_err(|e| self.io(e))
    }
}

#[cfg(unix)]
fn open_private(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), SecretError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .map_err(|source| SecretError::Io {
            path: path.to_owned(),
            source,
        })?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(SecretError::InsecurePermissions {
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path) -> Result<(), SecretError> {
    Ok(())
}

fn env_keys(name: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if name == CURSOR_API_KEY {
        keys.push("CURSOR_API_KEY".to_owned());
    }
    keys.push(format!(
        "LIVSTYLE_{}",
        name.replace('.', "_").to_uppercase()
    ));
    keys
}

/// `.env` inside the livstyle tree (cwd upward, stopping at the `livstyle`
/// directory), then `$XDG_CONFIG_HOME/livstyle/env`.
pub fn env_file_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(explicit) = std::env::var_os("LIVSTYLE_ENV_FILE") {
        out.push(PathBuf::from(explicit));
    }
    if let Ok(cwd) = std::env::current_dir() {
        let inside = cwd.components().any(|c| c.as_os_str() == "livstyle");
        let mut dir = Some(cwd.as_path());
        while let Some(current) = dir {
            out.push(current.join(".env"));
            if !inside || current.file_name().is_some_and(|n| n == "livstyle") {
                break;
            }
            dir = current.parent();
        }
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    {
        out.push(base.join("livstyle").join("env"));
    }
    out
}

/// Creates a 0600 env file when none of the candidates exist yet.
pub fn ensure_env_file() -> Result<PathBuf, SecretError> {
    for path in env_file_candidates() {
        if path.exists() {
            return Ok(path);
        }
    }
    let path = std::env::current_dir()
        .map(|d| d.join(".env"))
        .unwrap_or_else(|_| PathBuf::from(".env"));
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|source| SecretError::Io {
                path: path.clone(),
                source,
            })?;
        }
    }
    write_private(
        &path,
        "# Cursor API key for the SDK bridge. This file is gitignored.\n\
         # Write the key after the equals sign. Do not commit it.\n\
         CURSOR_API_KEY=\n",
    )?;
    Ok(path)
}

fn write_private(path: &Path, text: &str) -> Result<(), SecretError> {
    let mut file = open_private(path).map_err(|source| SecretError::Io {
        path: path.to_owned(),
        source,
    })?;
    file.write_all(text.as_bytes())
        .map_err(|source| SecretError::Io {
            path: path.to_owned(),
            source,
        })
}

fn read_env_files() -> Result<Vec<(PathBuf, BTreeMap<String, String>)>, SecretError> {
    let mut out = Vec::new();
    for path in env_file_candidates() {
        if !path.exists() {
            continue;
        }
        check_permissions(&path)?;
        let text = fs::read_to_string(&path).map_err(|source| SecretError::Io {
            path: path.clone(),
            source,
        })?;
        out.push((path, parse_env(&text)));
    }
    Ok(out)
}

fn parse_env(text: &str) -> BTreeMap<String, String> {
    let mut pairs = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        pairs.insert(key.to_owned(), value.to_owned());
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> CredentialStore {
        let dir =
            std::env::temp_dir().join(format!("livstyle-secrets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        CredentialStore::at(dir.join("credentials.toml"))
    }

    #[test]
    fn debug_never_reveals() {
        let s = Secret::new("key_abcdef123456");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert_eq!(s.fingerprint(), "key_…56");
    }

    #[test]
    fn round_trip_with_private_mode() {
        let store = store("rt");
        store
            .set("test.key", &Secret::new(" key_x123456789 \n"))
            .unwrap();
        assert_eq!(
            store.get("test.key").unwrap().unwrap().expose(),
            "key_x123456789"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(store.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(store.remove("test.key").unwrap());
        assert!(store.get("test.key").unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_world_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let store = store("perm");
        store.set("a", &Secret::new("value")).unwrap();
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            store.get("a"),
            Err(SecretError::InsecurePermissions { .. })
        ));
    }

    #[test]
    fn env_file_supplies_cursor_key() {
        let dir = std::env::temp_dir().join(format!("livstyle-env-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        write_private(&path, "export CURSOR_API_KEY=\"key_fromfile99\"\n# note\n").unwrap();
        let parsed = parse_env(&fs::read_to_string(&path).unwrap());
        assert_eq!(
            parsed.get("CURSOR_API_KEY").map(String::as_str),
            Some("key_fromfile99")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_secret_rejected() {
        assert!(matches!(
            store("empty").set("a", &Secret::new("  ")),
            Err(SecretError::Empty(_))
        ));
    }
}
