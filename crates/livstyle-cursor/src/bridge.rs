use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use livstyle_secrets::Secret;

use crate::ready::READY_PREFIX;
use crate::{CursorError, Endpoint, ReadyInfo, parse_ready_line};

/// Bridge release pinned by ADR-003; upgrades go through the ADR.
pub const PINNED_BRIDGE_VERSION: &str = "v1.0.38";

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const STDERR_TAIL: usize = 200;

/// Locates the bridge executable: `LIVSTYLE_CURSOR_BRIDGE_BIN`, then
/// `$XDG_DATA_HOME/livstyle/cursor-sdk-bridge/<pinned>/bin/cursor-sdk-bridge`.
/// The archive's `manifest.json` must declare `protocol == "sdk.v1"`.
pub fn locate_bridge() -> Result<PathBuf, CursorError> {
    if let Some(bin) = std::env::var_os("LIVSTYLE_CURSOR_BRIDGE_BIN") {
        return Ok(PathBuf::from(bin));
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| CursorError::Launch("cannot resolve data directory".into()))?;
    let root = data
        .join("livstyle/cursor-sdk-bridge")
        .join(PINNED_BRIDGE_VERSION);
    let exe = if cfg!(windows) {
        "bin/cursor-sdk-bridge.exe"
    } else {
        "bin/cursor-sdk-bridge"
    };
    let bin = root.join(exe);
    if !bin.exists() {
        return Err(CursorError::Launch(format!(
            "bridge {PINNED_BRIDGE_VERSION} not installed at {}; run scripts/fetch-cursor-bridge.sh",
            root.display()
        )));
    }
    check_manifest(&root)?;
    Ok(bin)
}

fn check_manifest(root: &Path) -> Result<(), CursorError> {
    let text = std::fs::read_to_string(root.join("manifest.json"))
        .map_err(|e| CursorError::Launch(format!("bridge manifest: {e}")))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| CursorError::Launch(format!("bridge manifest: {e}")))?;
    if manifest.get("protocol").and_then(|p| p.as_str()) != Some("sdk.v1") {
        return Err(CursorError::Launch(
            "bridge manifest protocol is not sdk.v1".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub binary: PathBuf,
    pub workspace: PathBuf,
    pub state_root: Option<PathBuf>,
}

/// A spawned bridge process. Dropping it shuts the bridge down.
pub struct Bridge {
    child: Option<Child>,
    endpoint: Endpoint,
    ready: ReadyInfo,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridge")
            .field("pid", &self.ready.pid)
            .field("server_version", &self.ready.server_version)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Bridge {
    pub fn spawn(config: &BridgeConfig, api_key: Option<&Secret>) -> Result<Self, CursorError> {
        let mut cmd = Command::new(&config.binary);
        cmd.arg("--workspace")
            .arg(&config.workspace)
            .env("CURSOR_SDK_CLIENT_LANGUAGE", "rust")
            // The auth token is written with the process temp dir. /tmp on this
            // machine is tmpfs with a user quota, so a full quota fails startup
            // with EDQUOT before the ready line. Prefer the user runtime dir.
            .env("TMPDIR", bridge_tmpdir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(root) = &config.state_root {
            cmd.arg("--state-root").arg(root);
        }
        match api_key {
            Some(key) => cmd.env("CURSOR_API_KEY", key.expose()),
            None => cmd.env_remove("CURSOR_API_KEY"),
        };
        let mut child = cmd
            .spawn()
            .map_err(|e| CursorError::Launch(format!("spawn {}: {e}", config.binary.display())))?;

        let stderr = child.stderr.take().expect("stderr piped");
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx) = mpsc::channel();
        {
            let tail = tail.clone();
            // Keep draining stderr for the bridge's whole life: a full pipe blocks it.
            thread::spawn(move || {
                let mut tx = Some(tx);
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    if let Some(parsed) = parse_ready_line(&line) {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(parsed);
                        }
                        // Never retain the ready line: older bridges inline the token.
                        continue;
                    }
                    let mut t = tail.lock().expect("stderr tail");
                    if t.len() == STDERR_TAIL {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            });
        }

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let ready = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(parsed) => break parsed,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = child.try_wait() {
                        return Err(CursorError::Launch(format!(
                            "bridge exited with {status} before ready: {}",
                            joined(&tail)
                        )));
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(CursorError::Launch(format!(
                            "bridge not ready after {STARTUP_TIMEOUT:?}: {}",
                            joined(&tail)
                        )));
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = child.wait();
                    return Err(CursorError::Launch(format!(
                        "bridge closed stderr before ready: {}",
                        joined(&tail)
                    )));
                }
            }
        };
        let ready = match ready {
            Ok(r) => r,
            Err(e) => {
                let _ = child.kill();
                return Err(e);
            }
        };

        let token = match (&ready.auth_token, &ready.auth_token_file) {
            (Some(inline), _) => inline.clone(),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| CursorError::Launch(format!("read auth token file: {e}")))?,
            (None, None) => {
                let _ = child.kill();
                return Err(CursorError::Launch("ready line has no auth token".into()));
            }
        };
        let endpoint = Endpoint::attach(&ready.endpoint_url().expect("validated"), &token)?;
        Ok(Self {
            child: Some(child),
            endpoint,
            ready,
            stderr_tail: tail,
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn ready_info(&self) -> &ReadyInfo {
        &self.ready
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .expect("stderr tail")
            .iter()
            .cloned()
            .collect()
    }

    pub fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|c| matches!(c.try_wait(), Ok(None)))
    }

    /// Graceful `Shutdown` RPC, wait, then kill.
    pub fn shutdown(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = self.endpoint.unary::<_, serde_json::Value>(
            "SdkBridgeControlService",
            "Shutdown",
            &serde_json::json!({ "graceSeconds": 2 }),
            Some(Duration::from_secs(3)),
        );
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// First directory that can actually be created and written. A mount can
/// report free space and still reject the write with `EDQUOT`.
fn pick_writable_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    for dir in candidates {
        if std::fs::create_dir_all(dir).is_err() {
            continue;
        }
        let probe = dir.join(".livstyle-write-probe");
        if std::fs::write(&probe, b"ok").is_ok() {
            let _ = std::fs::remove_file(&probe);
            return Some(dir.clone());
        }
    }
    None
}

fn bridge_tmpdir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        candidates.push(PathBuf::from(runtime).join("livstyle"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/share/livstyle/tmp"));
    }
    candidates.push(std::env::temp_dir().join("livstyle"));
    pick_writable_dir(&candidates).unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::pick_writable_dir;

    #[test]
    fn skips_a_directory_that_cannot_be_written() {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir);
        let root = base.join(format!("livstyle-tmpdir-{}", std::process::id()));
        let blocked = root.join("blocked");
        let ok = root.join("ok");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&blocked, b"not a directory").unwrap();
        let picked = pick_writable_dir(&[blocked, ok.clone()]).unwrap();
        assert_eq!(picked, ok);
        let _ = std::fs::remove_dir_all(&root);
    }
}

fn joined(tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    let t = tail.lock().expect("stderr tail");
    let lines: Vec<&str> = t
        .iter()
        .map(String::as_str)
        .filter(|l| !l.starts_with(READY_PREFIX))
        .collect();
    lines.join(" | ")
}
