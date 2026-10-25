use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use livstyle_http::Url;
use serde_json::{Value, json};
use thiserror::Error;

use crate::ws::Socket;

pub const FEATURE_TURN_STEER_DROPPED: &str = "event.turn_steer_dropped.v1";
pub const FEATURE_SESSION_HYDRATE: &str = "state.session_hydrate.v1";
pub const FEATURE_TASK_CONTROL: &str = "harness.task_control.v1";

const DEFAULT_FEATURES: &[&str] = &[
    FEATURE_TURN_STEER_DROPPED,
    FEATURE_SESSION_HYDRATE,
    FEATURE_TASK_CONTROL,
    "projection.envelope.v2",
    "session.workspace_cwd.v1",
];

#[derive(Debug, Error)]
pub enum OctosError {
    #[error("{0}")]
    Transport(String),
    #[error("OUP {method} failed: {message}")]
    Rpc { method: String, message: String },
    #[error("timed out waiting for {0}")]
    Timeout(String),
    #[error("connection closed")]
    Closed,
}

impl OctosError {
    /// Socket, timeout, or a reply that only exists because the reader
    /// closed the connection. A normal JSON-RPC error is not a link failure.
    pub fn is_link_failure(&self) -> bool {
        match self {
            OctosError::Transport(_) | OctosError::Closed | OctosError::Timeout(_) => true,
            OctosError::Rpc { message, .. } => message.contains("connection closed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub server_version: String,
    pub features: Vec<String>,
    pub methods: Vec<String>,
    pub raw: Value,
}

impl Hello {
    pub fn has_method(&self, method: &str) -> bool {
        self.methods.iter().any(|m| m == method)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// What livstyle's steering router needs from this runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerCaps {
    pub steer: bool,
    pub interrupt: bool,
    pub task_cancel: bool,
    pub restart_from_node: bool,
    pub steer_dropped: bool,
}

impl ServerCaps {
    pub fn from_hello(hello: &Hello) -> Self {
        Self {
            steer: hello.has_method("turn/steer"),
            interrupt: hello.has_method("turn/interrupt"),
            task_cancel: hello.has_method("task/cancel"),
            restart_from_node: hello.has_method("task/restart_from_node"),
            steer_dropped: hello.has_feature(FEATURE_TURN_STEER_DROPPED)
                || hello.has_method("turn/steer_dropped"),
        }
    }
}

struct Shared {
    write: Mutex<Socket>,
    pending: Mutex<HashMap<String, Sender<Result<Value, String>>>>,
    notes: Mutex<Vec<Value>>,
    next_id: Mutex<u64>,
}

/// One OUP connection. Requests are synchronous; notifications queue until
/// [`OctosClient::drain`].
pub struct OctosClient {
    shared: Arc<Shared>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl OctosClient {
    pub fn connect(endpoint: &str, token: &str) -> Result<Self, OctosError> {
        let httpish = endpoint.replacen("ws://", "http://", 1);
        let url = Url::parse(&httpish).map_err(|e| OctosError::Transport(e.to_string()))?;
        let socket =
            Socket::connect(&url, token, Duration::from_secs(5)).map_err(OctosError::Transport)?;
        let reader_socket = socket.try_clone().map_err(OctosError::Transport)?;
        let shared = Arc::new(Shared {
            write: Mutex::new(socket),
            pending: Mutex::new(HashMap::new()),
            notes: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        });
        let shared_reader = shared.clone();
        let reader = std::thread::spawn(move || read_loop(reader_socket, shared_reader));
        Ok(Self {
            shared,
            reader: Some(reader),
        })
    }

    pub fn hello(&self) -> Result<Hello, OctosError> {
        let raw = self.request(
            "client_hello",
            json!({
                "client": "livstyle",
                "supported_features": DEFAULT_FEATURES,
            }),
        )?;
        Ok(parse_hello(raw))
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value, OctosError> {
        let id = {
            let mut n = self.shared.next_id.lock().expect("id");
            *n += 1;
            n.to_string()
        };
        let (tx, rx) = mpsc::channel();
        self.shared
            .pending
            .lock()
            .expect("pending")
            .insert(id.clone(), tx);
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.shared
            .write
            .lock()
            .expect("write")
            .send_text(&frame.to_string())
            .map_err(OctosError::Transport)?;
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(message)) => Err(OctosError::Rpc {
                method: method.into(),
                message,
            }),
            Err(_) => Err(OctosError::Timeout(method.into())),
        }
    }

    /// Creates a local profile. `created: false` means it already existed.
    /// Points a profile at a loopback OpenAI-compatible model and makes it primary.
    pub fn upsert_primary_llm(
        &self,
        profile_id: &str,
        family_id: &str,
        model_id: &str,
        base_url: &str,
    ) -> Result<Value, OctosError> {
        self.request(
            "profile/llm/upsert",
            json!({
                "profile_id": profile_id,
                "set_primary": true,
                "selection": {
                    "family_id": family_id,
                    "model_id": model_id,
                    "route": {
                        "route_id": "livstyle-local",
                        "label": "livstyle",
                        "base_url": base_url,
                        "api_type": "openai"
                    }
                }
            }),
        )
    }

    pub fn create_profile(&self, requested_id: &str) -> Result<Value, OctosError> {
        self.request(
            "profile/local/create",
            json!({
                "requested_id": requested_id,
                "name": "livstyle",
                "make_default": true,
            }),
        )
    }

    pub fn open_session(
        &self,
        session_id: &str,
        profile_id: &str,
        cwd: &str,
    ) -> Result<Value, OctosError> {
        self.request(
            "session/open",
            json!({
                "session_id": session_id,
                "profile_id": profile_id,
                "cwd": cwd,
            }),
        )
    }

    pub fn hydrate(&self, session_id: &str) -> Result<Value, OctosError> {
        self.request(
            "session/hydrate",
            json!({ "session_id": session_id, "include": ["messages"] }),
        )
    }

    pub fn turn_start(
        &self,
        session_id: &str,
        turn_id: &str,
        text: &str,
    ) -> Result<Value, OctosError> {
        self.request(
            "turn/start",
            json!({
                "session_id": session_id,
                "turn_id": turn_id,
                "input": [{ "kind": "text", "text": text }],
            }),
        )
    }

    pub fn steer(
        &self,
        session_id: &str,
        expected_turn_id: Option<&str>,
        text: &str,
    ) -> Result<Value, OctosError> {
        let mut params = json!({
            "session_id": session_id,
            "input": [{ "kind": "text", "text": text }],
        });
        if let Some(turn) = expected_turn_id {
            params["expected_turn_id"] = json!(turn);
        }
        self.request("turn/steer", params)
    }

    pub fn interrupt(&self, session_id: &str, turn_id: &str) -> Result<Value, OctosError> {
        self.request(
            "turn/interrupt",
            json!({ "session_id": session_id, "turn_id": turn_id }),
        )
    }

    /// Notifications received since the last drain (server pushes, no id).
    pub fn drain(&self) -> Vec<Value> {
        std::mem::take(&mut *self.shared.notes.lock().expect("notes"))
    }
}

impl Drop for OctosClient {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            self.shared.write.lock().expect("write").shutdown();
            let _ = reader.join();
        }
    }
}

fn read_loop(mut socket: Socket, shared: Arc<Shared>) {
    let _ = socket.set_timeout(Some(Duration::from_secs(3600)));
    loop {
        match socket.recv() {
            Ok(Some(bytes)) => {
                let Ok(frame) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if let Some(id) = frame.get("id").and_then(Value::as_str) {
                    let reply = if let Some(error) = frame.get("error") {
                        Err(error.to_string())
                    } else {
                        Ok(frame.get("result").cloned().unwrap_or(Value::Null))
                    };
                    if let Some(tx) = shared.pending.lock().expect("pending").remove(id) {
                        let _ = tx.send(reply);
                    }
                } else if frame.get("method").is_some() {
                    shared.notes.lock().expect("notes").push(frame);
                }
            }
            Ok(None) | Err(_) => {
                let message = "connection closed".to_owned();
                for (_, tx) in shared.pending.lock().expect("pending").drain() {
                    let _ = tx.send(Err(message.clone()));
                }
                break;
            }
        }
    }
}

fn parse_hello(raw: Value) -> Hello {
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let caps = raw.get("capabilities");
    Hello {
        server_version: raw
            .pointer("/capabilities/version/protocol")
            .or_else(|| raw.get("server_version"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        features: strings(
            caps.and_then(|c| c.get("supported_features"))
                .or_else(|| raw.get("supported_features")),
        ),
        methods: strings(
            caps.and_then(|c| c.get("supported_methods"))
                .or_else(|| raw.get("supported_methods")),
        ),
        raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_follow_advertised_methods() {
        let hello = Hello {
            server_version: "x".into(),
            features: vec![FEATURE_TURN_STEER_DROPPED.into()],
            methods: vec![
                "turn/steer".into(),
                "turn/interrupt".into(),
                "task/cancel".into(),
            ],
            raw: Value::Null,
        };
        let caps = ServerCaps::from_hello(&hello);
        assert!(caps.steer && caps.interrupt && caps.task_cancel && caps.steer_dropped);
        assert!(!caps.restart_from_node);
    }

    #[test]
    fn link_failure_is_not_a_business_rpc_error() {
        assert!(OctosError::Closed.is_link_failure());
        assert!(OctosError::Timeout("turn/start".into()).is_link_failure());
        assert!(
            OctosError::Rpc {
                method: "turn/start".into(),
                message: "connection closed".into(),
            }
            .is_link_failure()
        );
        assert!(
            !OctosError::Rpc {
                method: "session/open".into(),
                message: "missing llm".into(),
            }
            .is_link_failure()
        );
    }
}
