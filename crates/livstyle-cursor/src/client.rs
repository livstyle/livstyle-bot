use std::path::Path;
use std::time::Duration;

use livstyle_secrets::Secret;
use livstyle_steering::RuntimeCaps;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{CursorError, Endpoint, RunStream, RunSummary};

const CONTROL: &str = "SdkBridgeControlService";
const AGENT: &str = "SdkAgentService";
const CATALOG: &str = "SdkCursorService";
const UNARY_TIMEOUT: Option<Duration> = Some(Duration::from_secs(60));

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    #[serde(default)]
    pub bridge_version: String,
    #[serde(default)]
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl VersionInfo {
    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// Steering capabilities for the intervention router (PRD §8.4). `sdk.v1`
/// has no mid-run injection; `run.cancel` enables interrupt-and-resend.
pub fn runtime_caps(version: &VersionInfo) -> RuntimeCaps {
    RuntimeCaps {
        steer: false,
        interrupt: version.has("run.cancel"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    #[serde(default)]
    pub api_key_name: String,
    #[serde(default)]
    pub user_email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHandle {
    pub agent_id: String,
    pub model: String,
}

/// Typed `sdk.v1` client. The API key is passed explicitly on every call
/// that accepts one, as the bridge docs require.
pub struct CursorClient {
    endpoint: Endpoint,
    api_key: Option<Secret>,
}

impl CursorClient {
    pub fn new(endpoint: Endpoint, api_key: Option<Secret>) -> Self {
        Self { endpoint, api_key }
    }

    fn key(&self) -> Result<&str, CursorError> {
        self.api_key
            .as_ref()
            .map(Secret::expose)
            .ok_or_else(|| CursorError::Auth {
                message: "no Cursor API key configured; run `livstyle auth cursor`".into(),
                details: None,
            })
    }

    pub fn ping(&self) -> Result<(), CursorError> {
        let _: Value = self
            .endpoint
            .unary(CONTROL, "Ping", &json!({}), UNARY_TIMEOUT)?;
        Ok(())
    }

    pub fn version(&self) -> Result<VersionInfo, CursorError> {
        let v: VersionInfo =
            self.endpoint
                .unary(CONTROL, "GetVersion", &json!({}), UNARY_TIMEOUT)?;
        if v.protocol_version != "sdk.v1" {
            return Err(CursorError::Protocol(format!(
                "bridge speaks {:?}, expected sdk.v1",
                v.protocol_version
            )));
        }
        Ok(v)
    }

    pub fn me(&self) -> Result<UserInfo, CursorError> {
        let v: Value = self.endpoint.unary(
            CATALOG,
            "Me",
            &json!({ "options": { "apiKey": self.key()? } }),
            UNARY_TIMEOUT,
        )?;
        serde_json::from_value(v.get("user").cloned().unwrap_or_default())
            .map_err(|e| CursorError::Protocol(format!("Me: {e}")))
    }

    pub fn list_models(&self) -> Result<Vec<ModelInfo>, CursorError> {
        let v: Value = self.endpoint.unary(
            CATALOG,
            "ListModels",
            &json!({ "options": { "apiKey": self.key()? } }),
            UNARY_TIMEOUT,
        )?;
        let items = v
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .into_iter()
            .filter_map(|m| {
                let id = m.get("id").and_then(Value::as_str)?.to_owned();
                let display_name = m
                    .get("displayName")
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Some(ModelInfo {
                    id,
                    display_name,
                    raw: m,
                })
            })
            .collect())
    }

    /// Local agent rooted at `cwd`.
    ///
    /// sdk.v1 rejects `idempotencyKey` here:
    /// `unimplemented: Idempotency-Key is only supported for cloud CreateAgent`.
    pub fn create_agent(
        &self,
        model: &str,
        cwd: &Path,
        name: Option<&str>,
    ) -> Result<AgentHandle, CursorError> {
        let req = local_create_agent_body(model, cwd, name, self.key()?);
        let v: Value = self
            .endpoint
            .unary(AGENT, "CreateAgent", &req, UNARY_TIMEOUT)?;
        Ok(AgentHandle {
            agent_id: v
                .get("agentId")
                .and_then(Value::as_str)
                .ok_or_else(|| CursorError::Protocol("CreateAgent: missing agentId".into()))?
                .to_owned(),
            model: v
                .pointer("/model/id")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_owned(),
        })
    }

    pub fn resume_agent(
        &self,
        agent_id: &str,
        model: &str,
        cwd: &Path,
    ) -> Result<AgentHandle, CursorError> {
        let req = json!({
            "agentId": agent_id,
            "options": {
                "model": { "id": model },
                "apiKey": self.key()?,
                "local": { "cwd": [cwd.to_string_lossy()] },
            }
        });
        let v: Value = self
            .endpoint
            .unary(AGENT, "ResumeAgent", &req, UNARY_TIMEOUT)?;
        Ok(AgentHandle {
            agent_id: v
                .get("agentId")
                .and_then(Value::as_str)
                .unwrap_or(agent_id)
                .to_owned(),
            model: v
                .pointer("/model/id")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_owned(),
        })
    }

    /// Starts a turn and returns the live stream.
    pub fn send(
        &self,
        agent_id: &str,
        text: &str,
        idempotency_key: Option<&str>,
    ) -> Result<RunStream, CursorError> {
        let mut req = json!({ "agentId": agent_id, "message": { "text": text } });
        if let Some(key) = idempotency_key {
            req["idempotencyKey"] = json!(key);
        }
        let frames = self.endpoint.server_stream(AGENT, "Send", &req)?;
        Ok(RunStream::new(frames, false))
    }

    /// Durable replay of a run, optionally after an offset from a previous
    /// `observe_run` stream.
    pub fn observe_run(
        &self,
        run_id: &str,
        after_offset: Option<&str>,
    ) -> Result<RunStream, CursorError> {
        let mut req = json!({ "runId": run_id });
        if let Some(offset) = after_offset {
            req["afterOffset"] = json!(offset);
        }
        let frames = self.endpoint.server_stream(AGENT, "ObserveRun", &req)?;
        Ok(RunStream::new(frames, true))
    }

    pub fn cancel_run(&self, run_id: &str, agent_id: Option<&str>) -> Result<(), CursorError> {
        let mut req = json!({ "runId": run_id });
        if let Some(agent) = agent_id {
            req["agentId"] = json!(agent);
        }
        let _: Value = self
            .endpoint
            .unary(AGENT, "CancelRun", &req, UNARY_TIMEOUT)?;
        Ok(())
    }

    pub fn wait_live_run(&self, run_id: &str) -> Result<RunSummary, CursorError> {
        let v: Value =
            self.endpoint
                .unary(AGENT, "WaitLiveRun", &json!({ "runId": run_id }), None)?;
        Ok(RunSummary::from_wire(v.get("result").unwrap_or(&v)))
    }

    pub fn close_agent(&self, agent_id: &str) -> Result<(), CursorError> {
        let _: Value = self.endpoint.unary(
            AGENT,
            "CloseAgent",
            &json!({ "agentId": agent_id }),
            UNARY_TIMEOUT,
        )?;
        Ok(())
    }
}

/// Status text for `Me`. A rejected API key and a rejected bridge token
/// are different problems and must not share one sentence.
pub fn account_status_line(err: &CursorError) -> String {
    match err {
        CursorError::BridgeUnauthenticated => {
            "Cursor  Bridge 拒绝了本地令牌，这不是 API Key。".into()
        }
        CursorError::Auth { message, .. } => format!("Cursor  API Key 未被接受：{message}"),
        other => format!("Cursor  账号没有确认：{other}"),
    }
}

/// Body for a local `CreateAgent`. Must not include `idempotencyKey`.
pub(crate) fn local_create_agent_body(
    model: &str,
    cwd: &Path,
    name: Option<&str>,
    api_key: &str,
) -> Value {
    let mut options = json!({
        "model": { "id": model },
        "apiKey": api_key,
        "local": { "cwd": [cwd.to_string_lossy()] },
    });
    if let Some(name) = name {
        options["name"] = json!(name);
    }
    json!({ "options": options })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_create_agent_omits_idempotency_key() {
        let body = local_create_agent_body(
            "composer-2",
            Path::new("/tmp/ws"),
            Some("livstyle"),
            "key",
        );
        assert!(body.get("idempotencyKey").is_none());
        assert_eq!(
            body.pointer("/options/local/cwd").cloned(),
            Some(json!(["/tmp/ws"]))
        );
        assert_eq!(
            body.pointer("/options/name").and_then(Value::as_str),
            Some("livstyle")
        );
    }

    #[test]
    fn caps_follow_bridge_capabilities() {
        let v = VersionInfo {
            bridge_version: "1.0.0".into(),
            protocol_version: "sdk.v1".into(),
            capabilities: vec!["agent.send".into(), "run.cancel".into()],
        };
        assert_eq!(
            runtime_caps(&v),
            RuntimeCaps {
                steer: false,
                interrupt: true
            }
        );
    }

    #[test]
    fn me_failure_is_not_reported_as_signed_in() {
        let key = account_status_line(&CursorError::Auth {
            message: "API_KEY_NOT_FOUND".into(),
            details: None,
        });
        let bridge = account_status_line(&CursorError::BridgeUnauthenticated);
        assert!(key.contains("API Key"));
        assert!(bridge.contains("本地令牌"));
        assert_ne!(key, bridge);
        assert!(!key.contains("已接通"));
        assert!(!bridge.contains("已接通"));
    }
}
