//! One chat progress vocabulary for every code agent.
//!
//! Cursor `sdk.v1` events and Octos OUP notifications both become [`TurnBeat`].
//! The selected agent can change only after the current turn ends. Cursor has
//! no mid-run injection, so text that arrives during its turn waits.

use livstyle_cursor::{RunEvent, RunStatus, RunSummary};
use livstyle_domain::TaskStatus;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeAgent {
    Cursor,
    Octos,
}

impl CodeAgent {
    pub fn label(self) -> &'static str {
        match self {
            CodeAgent::Cursor => "Cursor",
            CodeAgent::Octos => "Octos",
        }
    }

    pub fn runtime_id(self) -> &'static str {
        match self {
            CodeAgent::Cursor => "cursor",
            CodeAgent::Octos => "octos",
        }
    }

    pub fn can_steer(self) -> bool {
        matches!(self, CodeAgent::Octos)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnBeat {
    Append(String),
    Replace(String),
    Step(String),
    Done,
    Fail(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchPlan {
    Already,
    Now,
    AfterTurn,
}

pub fn plan_switch(busy: bool, current: CodeAgent, next: CodeAgent) -> SwitchPlan {
    if current == next {
        SwitchPlan::Already
    } else if busy {
        SwitchPlan::AfterTurn
    } else {
        SwitchPlan::Now
    }
}

/// What to do with a new chat line that arrives while a turn is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusyText {
    /// Octos can take the line inside the current turn.
    Steer,
    /// Cursor keeps the line for the next turn.
    Queue,
}

/// What the cancel button is allowed to do. It never starts another run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelEffect {
    /// Move the product task to Cancelled. Do not call the runtime again.
    MarkCancelled,
    /// The task already finished. Leave it, and do not send the cancel text.
    AlreadyTerminal,
}

pub fn cancel_effect(status: TaskStatus) -> CancelEffect {
    if status.is_terminal() {
        CancelEffect::AlreadyTerminal
    } else {
        CancelEffect::MarkCancelled
    }
}

pub fn busy_text(agent: CodeAgent) -> BusyText {
    if agent.can_steer() {
        BusyText::Steer
    } else {
        BusyText::Queue
    }
}

pub fn cursor_beats(event: &RunEvent) -> Vec<TurnBeat> {
    match event {
        RunEvent::Delta(value) => delta_beats(value),
        RunEvent::Step(value) => tool_step(value).into_iter().collect(),
        RunEvent::Message { kind, payload } => message_beats(kind, payload),
        RunEvent::Result(summary) => result_beats(summary),
        RunEvent::Done => vec![TurnBeat::Done],
    }
}

fn message_beats(kind: &str, payload: &Value) -> Vec<TurnBeat> {
    match kind {
        "assistant" => assistant_text(payload)
            .map(TurnBeat::Replace)
            .into_iter()
            .collect(),
        "tool_call" => tool_step(payload).into_iter().collect(),
        "thinking" | "reasoning" => thinking_text(payload)
            .map(|text| TurnBeat::Step(format!("思考 {text}")))
            .into_iter()
            .collect(),
        "status" => status_step(payload).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn assistant_text(payload: &Value) -> Option<String> {
    if let Some(text) = payload.get("text").and_then(Value::as_str) {
        let text = text.to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    let parts = payload
        .get("content")
        .or_else(|| payload.get("message"))
        .and_then(Value::as_array)?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.pointer("/text/text").and_then(Value::as_str))
        })
        .collect::<String>();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn thinking_text(payload: &Value) -> Option<String> {
    let text = payload
        .get("text")
        .or_else(|| payload.get("message"))
        .and_then(Value::as_str)
        .map(|text| clip(text, 160))?;
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn status_step(payload: &Value) -> Option<TurnBeat> {
    let text = payload
        .get("message")
        .and_then(Value::as_str)
        .map(|text| clip(text, 200))
        .unwrap_or_default();
    if text.is_empty() {
        return None;
    }
    Some(TurnBeat::Step(text))
}

fn delta_beats(value: &Value) -> Vec<TurnBeat> {
    let mut out = Vec::new();
    if let Some(text) = value
        .pointer("/textDelta/text")
        .or_else(|| value.pointer("/text_delta/text"))
        .and_then(Value::as_str)
    {
        if !text.is_empty() {
            out.push(TurnBeat::Append(text.to_string()));
        }
    } else if let Some(text) = value.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            out.push(TurnBeat::Append(text.to_string()));
        }
    }
    if let Some(text) = value
        .pointer("/thinkingDelta/text")
        .or_else(|| value.pointer("/thinking_delta/text"))
        .and_then(Value::as_str)
    {
        let text = clip(text, 160);
        if !text.is_empty() {
            out.push(TurnBeat::Step(format!("思考 {text}")));
        }
    }
    if let Some(step) = value
        .get("toolCallStarted")
        .or_else(|| value.get("tool_call_started"))
        .and_then(tool_step)
    {
        out.push(step);
    }
    if let Some(step) = value
        .get("toolCallCompleted")
        .or_else(|| value.get("tool_call_completed"))
        .and_then(tool_step)
    {
        out.push(step);
    }
    out
}

fn tool_step(value: &Value) -> Option<TurnBeat> {
    let name = value
        .get("name")
        .or_else(|| value.get("toolName"))
        .or_else(|| value.get("tool_name"))
        .or_else(|| value.pointer("/toolCall/name"))
        .or_else(|| value.pointer("/tool_call/name"))
        .and_then(Value::as_str)
        .unwrap_or("工具");
    let args = value
        .get("args")
        .or_else(|| value.get("arguments"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let args = clip(&args, 180);
    Some(TurnBeat::Step(if args.is_empty() {
        format!("开始 {name}")
    } else {
        format!("开始 {name}  {args}")
    }))
}

fn result_beats(summary: &RunSummary) -> Vec<TurnBeat> {
    match summary.status {
        RunStatus::Finished => {
            let mut out = Vec::new();
            if let Some(text) = summary
                .text
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                out.push(TurnBeat::Replace(text.to_string()));
            }
            out.push(TurnBeat::Done);
            out
        }
        RunStatus::Cancelled => vec![TurnBeat::Fail("这一轮已取消".into())],
        RunStatus::Error | RunStatus::Expired => {
            let message = summary
                .error_code
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Cursor 这一轮失败".into());
            vec![TurnBeat::Fail(message)]
        }
        _ => Vec::new(),
    }
}

pub fn octos_beats(note: &Value) -> Vec<TurnBeat> {
    let method = note.get("method").and_then(Value::as_str).unwrap_or("");
    let params = note.get("params").cloned().unwrap_or(Value::Null);
    let mut out = Vec::new();
    match method {
        "projection/envelope" => {
            let kind = params
                .pointer("/payload/type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let data = params
                .pointer("/payload/data")
                .cloned()
                .unwrap_or(Value::Null);
            match kind {
                "assistant_delta" => push_text(&mut out, &data, true),
                "assistant_persisted" => push_text(&mut out, &data, false),
                "reasoning_delta" => {
                    if let Some(text) = data.get("text").and_then(Value::as_str) {
                        let text = clip(text, 160);
                        if !text.is_empty() {
                            out.push(TurnBeat::Step(format!("思考 {text}")));
                        }
                    }
                }
                "tool_start" => {
                    let name = data.get("name").and_then(Value::as_str).unwrap_or("工具");
                    let args = data
                        .get("arguments_preview")
                        .and_then(Value::as_str)
                        .map(|s| clip(s, 180))
                        .unwrap_or_default();
                    out.push(TurnBeat::Step(if args.is_empty() {
                        format!("开始 {name}")
                    } else {
                        format!("开始 {name}  {args}")
                    }));
                }
                "tool_progress" => {
                    let message = data
                        .get("message")
                        .and_then(Value::as_str)
                        .map(|s| clip(s, 200))
                        .unwrap_or_default();
                    if !message.is_empty() {
                        out.push(TurnBeat::Step(format!("进行中 {message}")));
                    }
                }
                "tool_end" => {
                    let status = data.get("status").and_then(Value::as_str).unwrap_or("结束");
                    let preview = data
                        .get("output_preview")
                        .or_else(|| data.get("error"))
                        .and_then(Value::as_str)
                        .map(|s| clip(s, 220))
                        .unwrap_or_default();
                    out.push(TurnBeat::Step(if preview.is_empty() {
                        format!("结束 {status}")
                    } else {
                        format!("结束 {status}  {preview}")
                    }));
                }
                "turn_terminal" => {
                    let outcome = data.get("outcome").and_then(Value::as_str).unwrap_or("");
                    if outcome == "completed" || outcome.is_empty() {
                        out.push(TurnBeat::Done);
                    } else {
                        let message = data
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or(outcome);
                        out.push(TurnBeat::Fail(message.to_string()));
                    }
                }
                "background/spawn_complete" => {
                    if let Some(content) = data.get("content").and_then(Value::as_str) {
                        let content = content.trim();
                        if !content.is_empty() {
                            out.push(TurnBeat::Step("后台任务有结果".into()));
                            out.push(TurnBeat::Append(format!("\n{content}")));
                        }
                    }
                }
                _ => {}
            }
        }
        "message/delta" => push_text(&mut out, &params, true),
        "tool/started" => {
            let name = params
                .get("name")
                .or_else(|| params.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("工具");
            out.push(TurnBeat::Step(format!("开始 {name}")));
        }
        "approval/requested" => {
            let tool = params
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("操作");
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("需要批准");
            let body = params
                .get("body")
                .and_then(Value::as_str)
                .map(|s| clip(s, 240))
                .unwrap_or_default();
            out.push(TurnBeat::Step(format!("等待批准 {tool}：{title} {body}")));
        }
        "turn/error" => {
            let message = params
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("turn/error");
            out.push(TurnBeat::Fail(message.to_string()));
        }
        "turn/completed" => out.push(TurnBeat::Done),
        _ => {}
    }
    out
}

fn push_text(out: &mut Vec<TurnBeat>, data: &Value, append: bool) {
    let Some(text) = data.get("text").and_then(Value::as_str) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    out.push(if append {
        TurnBeat::Append(text.to_string())
    } else {
        TurnBeat::Replace(text.to_string())
    });
}

pub fn clip(text: &str, max_chars: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = text.chars().count();
    if count <= max_chars {
        text
    } else {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cancel_never_asks_for_another_run() {
        assert_eq!(
            cancel_effect(TaskStatus::Running),
            CancelEffect::MarkCancelled
        );
        assert_eq!(
            cancel_effect(TaskStatus::Queued),
            CancelEffect::MarkCancelled
        );
        assert_eq!(
            cancel_effect(TaskStatus::Succeeded),
            CancelEffect::AlreadyTerminal
        );
        assert_eq!(
            cancel_effect(TaskStatus::Failed),
            CancelEffect::AlreadyTerminal
        );
        assert_eq!(
            cancel_effect(TaskStatus::Cancelled),
            CancelEffect::AlreadyTerminal
        );
    }

    fn summary(status: RunStatus, text: Option<&str>, error: Option<&str>) -> RunSummary {
        RunSummary {
            agent_id: "a".into(),
            run_id: "r".into(),
            status,
            error_code: error.map(str::to_owned),
            text: text.map(str::to_owned),
            model: None,
            duration_ms: None,
            raw: Value::Null,
        }
    }

    #[test]
    fn cursor_text_and_tools_become_chat_beats() {
        let delta = cursor_beats(&RunEvent::Delta(json!({"textDelta": {"text": "he"}})));
        assert_eq!(delta, vec![TurnBeat::Append("he".into())]);

        let assistant = cursor_beats(&RunEvent::Message {
            kind: "assistant".into(),
            payload: json!({"text": "hello"}),
        });
        assert_eq!(assistant, vec![TurnBeat::Replace("hello".into())]);

        let tool = cursor_beats(&RunEvent::Delta(
            json!({"toolCallStarted": {"name": "edit", "args": "main.rs"}}),
        ));
        assert_eq!(tool, vec![TurnBeat::Step("开始 edit  main.rs".into())]);

        let done = cursor_beats(&RunEvent::Result(summary(
            RunStatus::Finished,
            Some("hello"),
            None,
        )));
        assert_eq!(
            done,
            vec![TurnBeat::Replace("hello".into()), TurnBeat::Done]
        );
    }

    #[test]
    fn cursor_failure_and_cancel_stop_the_turn() {
        let failed = cursor_beats(&RunEvent::Result(summary(
            RunStatus::Error,
            None,
            Some("usage_limit"),
        )));
        assert_eq!(failed, vec![TurnBeat::Fail("usage_limit".into())]);

        let cancelled = cursor_beats(&RunEvent::Result(summary(RunStatus::Cancelled, None, None)));
        assert_eq!(cancelled, vec![TurnBeat::Fail("这一轮已取消".into())]);
    }

    #[test]
    fn octos_deltas_stay_on_the_same_beats() {
        let note = json!({
            "method": "projection/envelope",
            "params": {"payload": {"type": "assistant_delta", "data": {"text": "hi"}}}
        });
        assert_eq!(octos_beats(&note), vec![TurnBeat::Append("hi".into())]);
    }

    #[test]
    fn switch_waits_for_the_turn_and_cursor_queues() {
        assert_eq!(
            plan_switch(false, CodeAgent::Cursor, CodeAgent::Octos),
            SwitchPlan::Now
        );
        assert_eq!(
            plan_switch(true, CodeAgent::Cursor, CodeAgent::Octos),
            SwitchPlan::AfterTurn
        );
        assert_eq!(
            plan_switch(true, CodeAgent::Cursor, CodeAgent::Cursor),
            SwitchPlan::Already
        );
        assert_eq!(busy_text(CodeAgent::Cursor), BusyText::Queue);
        assert_eq!(busy_text(CodeAgent::Octos), BusyText::Steer);
    }
}
