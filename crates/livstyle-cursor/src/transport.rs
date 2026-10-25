use std::io::Read;
use std::time::Duration;

use livstyle_http::{Request, Url, send};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::CursorError;
use crate::errors::from_connect_body;

/// An authenticated bridge endpoint (spawned or attached).
#[derive(Clone)]
pub struct Endpoint {
    url: Url,
    token: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.url)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl Endpoint {
    pub fn attach(url: &str, token: &str) -> Result<Self, CursorError> {
        Ok(Self {
            url: Url::parse(url).map_err(|e| CursorError::Launch(e.to_string()))?,
            token: token.trim().to_owned(),
        })
    }

    fn request<'a>(&'a self, url: &'a Url, content_type: &str, body: &'a [u8]) -> Request<'a> {
        Request::post(url, body)
            .header("Content-Type", content_type)
            .header("Connect-Protocol-Version", "1")
            .header("Authorization", format!("Bearer {}", self.token))
    }

    /// Connect unary RPC in JSON mode.
    pub fn unary<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        service: &str,
        method: &str,
        request: &Req,
        timeout: Option<Duration>,
    ) -> Result<Resp, CursorError> {
        let url = self.url.join(&format!("sdk.v1.{service}/{method}"));
        let body = serde_json::to_vec(request).map_err(|e| CursorError::Protocol(e.to_string()))?;
        let resp = send(
            self.request(&url, "application/json", &body)
                .read_timeout(timeout),
        )
        .map_err(|e| CursorError::Transport(format!("{service}/{method}: {e}")))?;
        let status = resp.status;
        let bytes = resp
            .read_all()
            .map_err(|e| CursorError::Transport(format!("{service}/{method}: {e}")))?;
        if !(200..300).contains(&status) {
            return Err(from_connect_body(&bytes, Some(status)));
        }
        let bytes = if bytes.is_empty() {
            b"{}".to_vec()
        } else {
            bytes
        };
        serde_json::from_slice(&bytes)
            .map_err(|e| CursorError::Protocol(format!("{service}/{method} response: {e}")))
    }

    /// Connect server-streaming RPC in JSON mode. Returns a frame reader.
    pub fn server_stream<Req: Serialize>(
        &self,
        service: &str,
        method: &str,
        request: &Req,
    ) -> Result<FrameReader, CursorError> {
        let url = self.url.join(&format!("sdk.v1.{service}/{method}"));
        let payload =
            serde_json::to_vec(request).map_err(|e| CursorError::Protocol(e.to_string()))?;
        let mut body = Vec::with_capacity(payload.len() + 5);
        body.push(0);
        body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        body.extend_from_slice(&payload);
        // No read timeout: runs idle during long tool calls; keepalives arrive every ~15s.
        let resp = send(
            self.request(&url, "application/connect+json", &body)
                .read_timeout(None),
        )
        .map_err(|e| CursorError::Transport(format!("{service}/{method}: {e}")))?;
        if !resp.is_success() {
            let status = resp.status;
            let bytes = resp.read_all().unwrap_or_default();
            return Err(from_connect_body(&bytes, Some(status)));
        }
        Ok(FrameReader {
            inner: Box::new(resp.into_reader()),
            finished: false,
        })
    }
}

/// Decodes Connect streaming envelopes: 1 flag byte + 4-byte BE length + payload.
pub struct FrameReader {
    inner: Box<dyn Read + Send>,
    finished: bool,
}

impl FrameReader {
    #[cfg(test)]
    pub(crate) fn from_reader(inner: impl Read + Send + 'static) -> Self {
        Self {
            inner: Box::new(inner),
            finished: false,
        }
    }

    /// Next message payload; `Ok(None)` after a clean end-of-stream frame.
    pub fn next_message(&mut self) -> Result<Option<serde_json::Value>, CursorError> {
        if self.finished {
            return Ok(None);
        }
        let mut head = [0u8; 5];
        self.inner
            .read_exact(&mut head)
            .map_err(|e| CursorError::Transport(format!("stream ended without end frame: {e}")))?;
        let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
        let mut payload = vec![0u8; len];
        self.inner
            .read_exact(&mut payload)
            .map_err(|e| CursorError::Transport(format!("truncated frame: {e}")))?;

        if head[0] & 0x02 != 0 {
            self.finished = true;
            if payload.is_empty() {
                return Ok(None);
            }
            let end: serde_json::Value = serde_json::from_slice(&payload)
                .map_err(|e| CursorError::Protocol(format!("end frame: {e}")))?;
            if let Some(error) = end.get("error") {
                return Err(from_connect_body(error.to_string().as_bytes(), None));
            }
            return Ok(None);
        }
        if head[0] & 0x01 != 0 {
            return Err(CursorError::Protocol(
                "compressed frames are not negotiated".into(),
            ));
        }
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(|e| CursorError::Protocol(format!("frame: {e}")))
    }
}

#[cfg(test)]
pub(crate) fn frame(flags: u8, json: &str) -> Vec<u8> {
    let mut v = vec![flags];
    v.extend_from_slice(&(json.len() as u32).to_be_bytes());
    v.extend_from_slice(json.as_bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_frames_until_end() {
        let mut bytes = frame(0, r#"{"sdkMessage":{"type":"system"}}"#);
        bytes.extend(frame(0, "{}"));
        bytes.extend(frame(2, "{}"));
        let mut r = FrameReader::from_reader(Cursor::new(bytes));
        assert!(
            r.next_message()
                .unwrap()
                .unwrap()
                .get("sdkMessage")
                .is_some()
        );
        assert_eq!(r.next_message().unwrap().unwrap(), serde_json::json!({}));
        assert!(r.next_message().unwrap().is_none());
        assert!(r.next_message().unwrap().is_none());
    }

    #[test]
    fn end_frame_error_is_surfaced() {
        let bytes = frame(2, r#"{"error":{"code":"not_found","message":"no run"}}"#);
        let mut r = FrameReader::from_reader(Cursor::new(bytes));
        assert!(matches!(
            r.next_message(),
            Err(CursorError::NotFound { .. })
        ));
    }

    #[test]
    fn missing_end_frame_is_transport_error() {
        let mut r = FrameReader::from_reader(Cursor::new(frame(0, "{}")));
        r.next_message().unwrap();
        assert!(matches!(r.next_message(), Err(CursorError::Transport(_))));
    }
}
