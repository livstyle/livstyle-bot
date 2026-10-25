use serde::Deserialize;

use crate::CursorError;

pub const READY_PREFIX: &str = "cursor-sdk-bridge ready ";

/// Discovery payload printed by the bridge on stderr once it listens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyInfo {
    pub schema_version: u32,
    pub server_version: String,
    pub pid: u32,
    pub transport: String,
    pub protocol: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub auth_token_file: Option<String>,
    /// Older bridges inline the token; never log this struct verbatim.
    #[serde(default)]
    pub auth_token: Option<String>,
    pub workspace_ref: Option<String>,
    pub state_root: Option<String>,
}

impl ReadyInfo {
    pub fn endpoint_url(&self) -> Option<String> {
        if let Some(url) = &self.url {
            return Some(url.clone());
        }
        let host = self.host.as_deref()?;
        let port = self.port?;
        Some(if host.contains(':') {
            format!("http://[{host}]:{port}")
        } else {
            format!("http://{host}:{port}")
        })
    }
}

/// `None` for ordinary diagnostic lines; `Some(Err)` for a ready line that
/// violates the discovery contract.
pub fn parse_ready_line(line: &str) -> Option<Result<ReadyInfo, CursorError>> {
    let json = line.strip_prefix(READY_PREFIX)?;
    let parsed = serde_json::from_str::<ReadyInfo>(json)
        .map_err(|e| CursorError::Launch(format!("unparseable ready line: {e}")))
        .and_then(|info| {
            if info.schema_version != 1 {
                return Err(CursorError::Launch(format!(
                    "unsupported discovery schemaVersion {}",
                    info.schema_version
                )));
            }
            if info.transport != "tcp" || info.protocol != "connect" {
                return Err(CursorError::Launch(format!(
                    "unsupported transport/protocol {}/{}",
                    info.transport, info.protocol
                )));
            }
            if info.endpoint_url().is_none() {
                return Err(CursorError::Launch(
                    "ready line has no url or host/port".into(),
                ));
            }
            Ok(info)
        });
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"cursor-sdk-bridge ready {"schemaVersion":1,"serverVersion":"1.0.0","pid":12345,"transport":"tcp","protocol":"connect","host":"127.0.0.1","port":49152,"url":"http://127.0.0.1:49152","authTokenFile":"/tmp/x/auth-token","workspaceRef":"/w","stateRoot":"/s","futureField":true}"#;

    #[test]
    fn parses_ready_line_and_ignores_unknown_fields() {
        let info = parse_ready_line(LINE).unwrap().unwrap();
        assert_eq!(info.endpoint_url().unwrap(), "http://127.0.0.1:49152");
        assert_eq!(info.auth_token_file.as_deref(), Some("/tmp/x/auth-token"));
    }

    #[test]
    fn ignores_diagnostics_and_rejects_bad_schema() {
        assert!(parse_ready_line("loading sdk...").is_none());
        let bad = LINE.replace("\"schemaVersion\":1", "\"schemaVersion\":2");
        assert!(parse_ready_line(&bad).unwrap().is_err());
        let ws = LINE.replace("\"tcp\"", "\"ws\"");
        assert!(parse_ready_line(&ws).unwrap().is_err());
    }

    #[test]
    fn falls_back_to_host_port() {
        let line = LINE
            .replace(r#""url":"http://127.0.0.1:49152","#, "")
            .replace("127.0.0.1", "::1");
        let info = parse_ready_line(&line).unwrap().unwrap();
        assert_eq!(info.endpoint_url().unwrap(), "http://[::1]:49152");
    }
}
