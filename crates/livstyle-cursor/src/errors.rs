use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

/// Stable error taxonomy from `sdk_errors.proto`. Unknown values are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkErrorCode {
    Unspecified,
    Unauthorized,
    ApiKeyNotFound,
    PlanRequired,
    RoleForbidden,
    FeatureUnavailable,
    AgentNotFound,
    RunNotFound,
    ValidationError,
    InvalidModel,
    InvalidBranchName,
    RepositoryRequired,
    RepositoryAccess,
    PrResolutionFailed,
    UsageLimitExceeded,
    AgentBusy,
    AgentArchived,
    RunNotCancellable,
    RateLimitExceeded,
    UpstreamError,
    InternalError,
    ClientCancelled,
    Unknown(u64),
}

impl SdkErrorCode {
    pub fn from_wire(v: u64) -> Self {
        use SdkErrorCode::*;
        match v {
            0 => Unspecified,
            1 => Unauthorized,
            2 => ApiKeyNotFound,
            3 => PlanRequired,
            4 => RoleForbidden,
            5 => FeatureUnavailable,
            6 => AgentNotFound,
            7 => RunNotFound,
            8 => ValidationError,
            9 => InvalidModel,
            10 => InvalidBranchName,
            11 => RepositoryRequired,
            12 => RepositoryAccess,
            13 => PrResolutionFailed,
            14 => UsageLimitExceeded,
            15 => AgentBusy,
            16 => AgentArchived,
            17 => RunNotCancellable,
            18 => RateLimitExceeded,
            19 => UpstreamError,
            20 => InternalError,
            21 => ClientCancelled,
            other => Unknown(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SdkErrorDetails {
    pub request_id: Option<String>,
    pub code: Option<SdkErrorCode>,
    pub message: String,
    pub help_url: Option<String>,
    pub provider: Option<String>,
    pub retry_after: Option<Duration>,
    pub rate_limit_remaining: Option<u64>,
    pub rate_limit_reset_epoch_seconds: Option<u64>,
}

#[derive(Debug, Error)]
pub enum CursorError {
    #[error("bridge launch failed: {0}")]
    Launch(String),
    #[error("transport: {0}")]
    Transport(String),
    /// Wrong or missing bridge bearer token. Fix the handshake, don't retry.
    #[error("bridge rejected the local bearer token")]
    BridgeUnauthenticated,
    /// Cursor rejected or lacks the API key (UNAUTHORIZED / API_KEY_NOT_FOUND).
    #[error("Cursor API key rejected: {message}")]
    Auth {
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    /// Plan, role or usage limits (PRD §9.2: report clearly, never silently switch).
    #[error("Cursor account limit: {message}")]
    Quota {
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("rate limited: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("agent busy: {message}")]
    Busy {
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("not found: {message}")]
    NotFound {
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("invalid request: {message}")]
    Invalid {
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("bridge error {code}: {message}")]
    Rpc {
        code: String,
        message: String,
        details: Option<Box<SdkErrorDetails>>,
    },
    #[error("protocol: {0}")]
    Protocol(String),
}

impl CursorError {
    pub fn details(&self) -> Option<&SdkErrorDetails> {
        match self {
            CursorError::Auth { details, .. }
            | CursorError::Quota { details, .. }
            | CursorError::RateLimited { details, .. }
            | CursorError::Busy { details, .. }
            | CursorError::NotFound { details, .. }
            | CursorError::Invalid { details, .. }
            | CursorError::Rpc { details, .. } => details.as_deref(),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct ConnectErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    details: Vec<ConnectErrorDetail>,
}

#[derive(Deserialize)]
struct ConnectErrorDetail {
    #[serde(default, rename = "type")]
    type_name: String,
    #[serde(default)]
    value: String,
}

/// Maps a Connect JSON error body onto the error taxonomy.
pub(crate) fn from_connect_body(body: &[u8], http_status: Option<u16>) -> CursorError {
    let Ok(parsed) = serde_json::from_slice::<ConnectErrorBody>(body) else {
        let text = String::from_utf8_lossy(body);
        return CursorError::Rpc {
            code: http_status.map_or("unknown".into(), |s| format!("http_{s}")),
            message: text.chars().take(500).collect(),
            details: None,
        };
    };
    let details = parsed
        .details
        .iter()
        .find(|d| d.type_name.ends_with("sdk.v1.SdkErrorDetails"))
        .and_then(|d| base64_decode(&d.value))
        .and_then(|bytes| decode_sdk_error_details(&bytes));
    classify(parsed.code, parsed.message, details)
}

fn classify(code: String, message: String, details: Option<SdkErrorDetails>) -> CursorError {
    use SdkErrorCode::*;
    let details = details.map(Box::new);
    if code == "unauthenticated" && details.is_none() && message == "Unauthorized" {
        return CursorError::BridgeUnauthenticated;
    }
    let message = details
        .as_ref()
        .filter(|d| !d.message.is_empty())
        .map_or(message, |d| d.message.clone());
    match details.as_ref().and_then(|d| d.code) {
        Some(Unauthorized | ApiKeyNotFound) => CursorError::Auth { message, details },
        Some(PlanRequired | RoleForbidden | FeatureUnavailable | UsageLimitExceeded) => {
            CursorError::Quota { message, details }
        }
        Some(RateLimitExceeded) => CursorError::RateLimited {
            retry_after: details.as_ref().and_then(|d| d.retry_after),
            message,
            details,
        },
        Some(AgentBusy) => CursorError::Busy { message, details },
        Some(AgentNotFound | RunNotFound) => CursorError::NotFound { message, details },
        Some(ValidationError | InvalidModel | InvalidBranchName) => {
            CursorError::Invalid { message, details }
        }
        _ => match code.as_str() {
            "unauthenticated" => CursorError::Auth { message, details },
            "not_found" => CursorError::NotFound { message, details },
            "invalid_argument" => CursorError::Invalid { message, details },
            "resource_exhausted" => CursorError::RateLimited {
                retry_after: None,
                message,
                details,
            },
            _ => CursorError::Rpc {
                code,
                message,
                details,
            },
        },
    }
}

/// Standard or URL-safe base64, padded or not.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

struct Wire<'a> {
    buf: &'a [u8],
}

impl<'a> Wire<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let (&b, rest) = self.buf.split_first()?;
            self.buf = rest;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.varint()? as usize;
        if len > self.buf.len() {
            return None;
        }
        let (head, rest) = self.buf.split_at(len);
        self.buf = rest;
        Some(head)
    }

    fn skip(&mut self, wire_type: u64) -> Option<()> {
        match wire_type {
            0 => self.varint().map(|_| ()),
            1 => self.advance(8),
            2 => self.bytes().map(|_| ()),
            5 => self.advance(4),
            _ => None,
        }
    }

    fn advance(&mut self, n: usize) -> Option<()> {
        if n > self.buf.len() {
            return None;
        }
        self.buf = &self.buf[n..];
        Some(())
    }

    /// Iterates `(field_number, wire_type)` and lets `f` consume the value.
    fn fields(mut self, mut f: impl FnMut(u64, u64, &mut Self) -> Option<()>) -> Option<()> {
        while !self.buf.is_empty() {
            let key = self.varint()?;
            f(key >> 3, key & 7, &mut self)?;
        }
        Some(())
    }
}

fn utf8(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Hand-decodes `sdk.v1.SdkErrorDetails` from protobuf bytes.
pub(crate) fn decode_sdk_error_details(buf: &[u8]) -> Option<SdkErrorDetails> {
    let mut d = SdkErrorDetails::default();
    Wire { buf }.fields(|field, wt, w| {
        match (field, wt) {
            (1, 2) => d.request_id = Some(utf8(w.bytes()?)),
            (2, 0) => d.code = Some(SdkErrorCode::from_wire(w.varint()?)),
            (3, 2) => d.message = utf8(w.bytes()?),
            (4, 2) => d.help_url = Some(utf8(w.bytes()?)),
            (5, 2) => d.provider = Some(utf8(w.bytes()?)),
            (6, 2) => {
                let (mut secs, mut nanos) = (0u64, 0u64);
                Wire { buf: w.bytes()? }.fields(|f, t, w| {
                    match (f, t) {
                        (1, 0) => secs = w.varint()?,
                        (2, 0) => nanos = w.varint()?,
                        _ => w.skip(t)?,
                    }
                    Some(())
                })?;
                d.retry_after = Some(Duration::new(secs, nanos as u32));
            }
            (7, 2) => {
                Wire { buf: w.bytes()? }.fields(|f, t, w| {
                    match (f, t) {
                        (2, 0) => d.rate_limit_remaining = Some(w.varint()?),
                        (3, 0) => d.rate_limit_reset_epoch_seconds = Some(w.varint()?),
                        _ => w.skip(t)?,
                    }
                    Some(())
                })?;
            }
            _ => w.skip(wt)?,
        }
        Some(())
    })?;
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_bytes(field: u8, data: &[u8]) -> Vec<u8> {
        let mut v = vec![(field << 3) | 2, data.len() as u8];
        v.extend_from_slice(data);
        v
    }

    fn sample_details() -> Vec<u8> {
        let mut buf = field_bytes(1, b"req-123");
        buf.extend([2 << 3, 18]); // RATE_LIMIT_EXCEEDED
        buf.extend(field_bytes(3, b"slow down"));
        buf.extend(field_bytes(6, &[1 << 3, 30])); // 30s
        buf.extend(field_bytes(7, &[2 << 3, 0, 3 << 3, 100]));
        buf
    }

    fn b64(bytes: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().fold(0u32, |a, &b| (a << 8) | b as u32) << (8 * (3 - chunk.len()));
            for i in 0..=chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn decodes_details_and_classifies_rate_limit() {
        let body = serde_json::json!({
            "code": "resource_exhausted",
            "message": "x",
            "details": [{"type": "sdk.v1.SdkErrorDetails", "value": b64(&sample_details())}]
        });
        let err = from_connect_body(body.to_string().as_bytes(), Some(429));
        let CursorError::RateLimited {
            retry_after,
            message,
            details,
        } = err
        else {
            panic!("wrong class: {err:?}");
        };
        assert_eq!(retry_after, Some(Duration::from_secs(30)));
        assert_eq!(message, "slow down");
        let d = details.unwrap();
        assert_eq!(d.request_id.as_deref(), Some("req-123"));
        assert_eq!(d.rate_limit_remaining, Some(0));
        assert_eq!(d.rate_limit_reset_epoch_seconds, Some(100));
    }

    #[test]
    fn bare_unauthorized_is_bridge_token_problem() {
        let err = from_connect_body(
            br#"{"code":"unauthenticated","message":"Unauthorized"}"#,
            Some(401),
        );
        assert!(matches!(err, CursorError::BridgeUnauthenticated));
        let err = from_connect_body(
            br#"{"code":"unauthenticated","message":"API key is required for cloud catalog calls."}"#,
            Some(401),
        );
        assert!(matches!(err, CursorError::Auth { .. }));
    }

    #[test]
    fn unknown_enum_values_survive() {
        let mut buf = vec![2 << 3, 99];
        buf.extend(field_bytes(3, b"new"));
        let d = decode_sdk_error_details(&buf).unwrap();
        assert_eq!(d.code, Some(SdkErrorCode::Unknown(99)));
    }

    #[test]
    fn base64_variants() {
        assert_eq!(base64_decode("aGk").unwrap(), b"hi");
        assert_eq!(base64_decode("aGk=").unwrap(), b"hi");
        assert_eq!(base64_decode("-_8").unwrap(), [0xfb, 0xff]);
    }
}
