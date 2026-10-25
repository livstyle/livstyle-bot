use serde_json::Value;

use crate::CursorError;
use crate::transport::FrameReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Creating,
    Running,
    Finished,
    Error,
    Cancelled,
    Expired,
    Unspecified,
}

impl RunStatus {
    pub fn from_wire(v: &Value) -> Self {
        let s = v.as_str().unwrap_or_default();
        match s.trim_start_matches("RUN_LIFECYCLE_STATUS_") {
            "CREATING" => RunStatus::Creating,
            "RUNNING" => RunStatus::Running,
            "FINISHED" => RunStatus::Finished,
            "ERROR" => RunStatus::Error,
            "CANCELLED" => RunStatus::Cancelled,
            "EXPIRED" => RunStatus::Expired,
            _ => RunStatus::Unspecified,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Finished | RunStatus::Error | RunStatus::Cancelled | RunStatus::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub agent_id: String,
    pub run_id: String,
    pub status: RunStatus,
    pub error_code: Option<String>,
    pub text: Option<String>,
    pub model: Option<String>,
    pub duration_ms: Option<u64>,
    pub raw: Value,
}

impl RunSummary {
    pub(crate) fn from_wire(v: &Value) -> Self {
        let result = v.get("result").cloned().unwrap_or(Value::Null);
        let str_at = |obj: &Value, k: &str| obj.get(k).and_then(Value::as_str).map(str::to_owned);
        Self {
            agent_id: str_at(v, "agentId")
                .or_else(|| str_at(&result, "agentId"))
                .unwrap_or_default(),
            run_id: str_at(v, "runId")
                .or_else(|| str_at(&result, "runId"))
                .unwrap_or_default(),
            status: RunStatus::from_wire(
                v.get("status")
                    .or_else(|| result.get("status"))
                    .unwrap_or(&Value::Null),
            ),
            error_code: str_at(v, "errorCode").filter(|s| !s.is_empty()),
            text: str_at(&result, "result"),
            model: result
                .pointer("/model/id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            duration_ms: result
                .get("durationMs")
                .and_then(|d| d.as_u64().or_else(|| d.as_str()?.parse().ok())),
            raw: v.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    /// `sdk_message`: `type` discriminator plus the JSON payload.
    Message {
        kind: String,
        payload: Value,
    },
    Delta(Value),
    Step(Value),
    Result(RunSummary),
    Done,
}

/// Iterator over a `Send` or `ObserveRun` stream. Keepalives and unknown
/// envelope cases are skipped; the last non-empty offset is tracked for resume.
pub struct RunStream {
    frames: FrameReader,
    last_offset: Option<String>,
    run_id: Option<String>,
    agent_id: Option<String>,
    last_status_message: Option<String>,
    durable: bool,
    done: bool,
}

impl RunStream {
    pub(crate) fn new(frames: FrameReader, durable: bool) -> Self {
        Self {
            frames,
            last_offset: None,
            run_id: None,
            agent_id: None,
            last_status_message: None,
            durable,
            done: false,
        }
    }

    /// Resume token, only meaningful for `ObserveRun` streams (see bridge
    /// streaming docs: live `Send` offsets use a different numbering).
    pub fn resume_offset(&self) -> Option<&str> {
        self.durable
            .then_some(self.last_offset.as_deref())
            .flatten()
    }

    pub fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    /// Human-readable failure text from the last `status` message; the
    /// terminal `error_code` can be empty for failed runs.
    pub fn last_status_message(&self) -> Option<&str> {
        self.last_status_message.as_deref()
    }

    fn note_ids(&mut self, payload: &Value) {
        for (key, slot) in [
            ("run_id", &mut self.run_id),
            ("agent_id", &mut self.agent_id),
        ] {
            if slot.is_none() {
                let camel = if key == "run_id" { "runId" } else { "agentId" };
                if let Some(v) = payload
                    .get(key)
                    .or_else(|| payload.get(camel))
                    .and_then(Value::as_str)
                {
                    *slot = Some(v.to_owned());
                }
            }
        }
    }

    /// Drains the stream and returns the terminal summary.
    pub fn wait(mut self) -> Result<RunSummary, CursorError> {
        let mut summary = None;
        for event in self.by_ref() {
            if let RunEvent::Result(r) = event? {
                summary = Some(r);
            }
        }
        summary.ok_or_else(|| CursorError::Protocol("stream ended without a result".into()))
    }
}

impl Iterator for RunStream {
    type Item = Result<RunEvent, CursorError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return None;
            }
            let msg = match self.frames.next_message() {
                Ok(Some(m)) => m,
                Ok(None) => {
                    self.done = true;
                    return None;
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };
            if let Some(offset) = msg.get("offset").and_then(Value::as_str) {
                if !offset.is_empty() {
                    self.last_offset = Some(offset.to_owned());
                }
            }
            if let Some(m) = msg.get("sdkMessage") {
                let kind = m
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let payload = m.get("message").cloned().unwrap_or(Value::Null);
                self.note_ids(&payload);
                if kind == "status" {
                    if let Some(text) = payload.get("message").and_then(Value::as_str) {
                        self.last_status_message = Some(text.to_owned());
                    }
                }
                return Some(Ok(RunEvent::Message { kind, payload }));
            }
            if let Some(r) = msg.get("result") {
                let summary = RunSummary::from_wire(r);
                if self.run_id.is_none() && !summary.run_id.is_empty() {
                    self.run_id = Some(summary.run_id.clone());
                }
                return Some(Ok(RunEvent::Result(summary)));
            }
            if msg.get("done").is_some() {
                return Some(Ok(RunEvent::Done));
            }
            if let Some(d) = msg.get("interactionUpdate") {
                return Some(Ok(RunEvent::Delta(d.clone())));
            }
            if let Some(s) = msg.get("step") {
                return Some(Ok(RunEvent::Step(s.clone())));
            }
            // Keepalive or unknown envelope case: skip silently.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::frame;
    use std::io::Cursor;

    fn stream(frames: &[(u8, &str)], durable: bool) -> RunStream {
        let mut bytes = Vec::new();
        for (flags, json) in frames {
            bytes.extend(frame(*flags, json));
        }
        RunStream::new(FrameReader::from_reader(Cursor::new(bytes)), durable)
    }

    #[test]
    fn typical_live_stream() {
        let s = stream(
            &[
                (
                    0,
                    r#"{"sdkMessage":{"type":"system","message":{"subtype":"init","run_id":"run-1","agent_id":"agent-1"}},"offset":"1"}"#,
                ),
                (0, "{}"),
                (0, r#"{"futureCase":{}}"#),
                (
                    0,
                    r#"{"sdkMessage":{"type":"assistant","message":{"text":"hi"}},"offset":"2"}"#,
                ),
                (
                    0,
                    r#"{"result":{"agentId":"agent-1","runId":"run-1","status":"RUN_LIFECYCLE_STATUS_FINISHED","result":{"result":"hello","durationMs":"1200","model":{"id":"composer-2"}}}}"#,
                ),
                (0, r#"{"done":{}}"#),
                (2, "{}"),
            ],
            false,
        );
        let events: Vec<_> = s.map(Result::unwrap).collect();
        assert_eq!(events.len(), 4);
        let RunEvent::Result(r) = &events[2] else {
            panic!()
        };
        assert_eq!(r.status, RunStatus::Finished);
        assert_eq!(r.text.as_deref(), Some("hello"));
        assert_eq!(r.duration_ms, Some(1200));
        assert_eq!(r.model.as_deref(), Some("composer-2"));
    }

    #[test]
    fn offsets_only_exposed_for_durable_streams() {
        let frames = [
            (
                0,
                r#"{"sdkMessage":{"type":"status","message":{"status":"ERROR","message":"model failed","run_id":"r"}},"offset":"7"}"#,
            ),
            (0, r#"{}"#),
            (2, "{}"),
        ];
        let mut live = stream(&frames, false);
        live.next();
        assert_eq!(live.resume_offset(), None);
        assert_eq!(live.last_status_message(), Some("model failed"));

        let mut durable = stream(&frames, true);
        durable.next();
        durable.next();
        assert_eq!(durable.resume_offset(), Some("7"));
        assert_eq!(durable.run_id(), Some("r"));
    }
}
