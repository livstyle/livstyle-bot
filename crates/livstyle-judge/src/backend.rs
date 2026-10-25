use std::time::Duration;

use livstyle_http::{Request, Url, send};
use serde_json::{Value, json};
use thiserror::Error;

/// `(token, logprob)` candidates for the first generated token.
pub type TopLogprobs = Vec<(String, f64)>;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend unreachable: {0}")]
    Unavailable(String),
    #[error("backend returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("backend response has no logprobs (unsupported model or server)")]
    NoLogprobs,
    #[error("malformed backend response: {0}")]
    Malformed(String),
}

pub trait Backend: Send + Sync {
    /// Human-readable identity recorded with every judgment.
    fn model_id(&self) -> String;
    /// Next-token distribution after `system` + `user` with the assistant turn
    /// primed to answer immediately.
    fn top_logprobs(&self, system: &str, user: &str) -> Result<TopLogprobs, BackendError>;
}

fn post_json(url: &Url, body: &Value, timeout: Duration) -> Result<Value, BackendError> {
    let bytes = serde_json::to_vec(body).expect("json");
    let resp = send(
        Request::post(url, &bytes)
            .header("Content-Type", "application/json")
            .read_timeout(Some(timeout)),
    )
    .map_err(|e| BackendError::Unavailable(e.to_string()))?;
    let status = resp.status;
    let raw = resp
        .read_all()
        .map_err(|e| BackendError::Unavailable(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(BackendError::Http {
            status,
            body: String::from_utf8_lossy(&raw).chars().take(300).collect(),
        });
    }
    serde_json::from_slice(&raw).map_err(|e| BackendError::Malformed(e.to_string()))
}

fn parse_top(entries: Option<&Value>) -> Result<TopLogprobs, BackendError> {
    let entries = entries
        .and_then(Value::as_array)
        .ok_or(BackendError::NoLogprobs)?;
    let out: TopLogprobs = entries
        .iter()
        .filter_map(|e| {
            Some((
                e.get("token")?.as_str()?.to_owned(),
                e.get("logprob")?.as_f64()?,
            ))
        })
        .collect();
    if out.is_empty() {
        return Err(BackendError::NoLogprobs);
    }
    Ok(out)
}

/// Ollama native `/api/generate` in raw mode with a ChatML prompt. Raw mode
/// lets us prime the assistant turn (empty think block + `Answer:`), which
/// the OpenAI-compatible endpoint cannot do.
#[derive(Debug, Clone)]
pub struct OllamaRaw {
    pub base: Url,
    pub model: String,
    pub top_k: u32,
    pub timeout: Duration,
}

impl OllamaRaw {
    pub fn new(base_url: &str, model: &str) -> Result<Self, BackendError> {
        Ok(Self {
            base: Url::parse(base_url).map_err(|e| BackendError::Unavailable(e.to_string()))?,
            model: model.to_owned(),
            top_k: 20,
            timeout: Duration::from_secs(120),
        })
    }

    pub fn chatml(system: &str, user: &str) -> String {
        format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\nAnswer:"
        )
    }
}

impl Backend for OllamaRaw {
    fn model_id(&self) -> String {
        format!("ollama:{}", self.model)
    }

    fn top_logprobs(&self, system: &str, user: &str) -> Result<TopLogprobs, BackendError> {
        let body = json!({
            "model": self.model,
            "prompt": Self::chatml(system, user),
            "raw": true,
            "stream": false,
            "logprobs": true,
            "top_logprobs": self.top_k,
            "keep_alive": "30m",
            "options": { "num_predict": 1, "temperature": 0 },
        });
        let v = post_json(&self.base.join("api/generate"), &body, self.timeout)?;
        parse_top(v.pointer("/logprobs/0/top_logprobs"))
    }
}

/// OpenAI-compatible `chat/completions` with `logprobs` (llama-server `/v1`,
/// vLLM, Ollama `/v1`). The server applies its own chat template, so the
/// first token may be whitespace; prefer [`OllamaRaw`] when available.
#[derive(Debug, Clone)]
pub struct OpenAiCompat {
    pub base: Url,
    pub model: String,
    pub top_k: u32,
    pub timeout: Duration,
}

impl OpenAiCompat {
    pub fn new(base_url: &str, model: &str) -> Result<Self, BackendError> {
        Ok(Self {
            base: Url::parse(base_url).map_err(|e| BackendError::Unavailable(e.to_string()))?,
            model: model.to_owned(),
            top_k: 20,
            timeout: Duration::from_secs(120),
        })
    }
}

impl Backend for OpenAiCompat {
    fn model_id(&self) -> String {
        format!("openai-compat:{}", self.model)
    }

    fn top_logprobs(&self, system: &str, user: &str) -> Result<TopLogprobs, BackendError> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": format!("{user}\nReply with the option letter only.") },
            ],
            "max_tokens": 1,
            "temperature": 0,
            "logprobs": true,
            "top_logprobs": self.top_k,
        });
        let v = post_json(&self.base.join("chat/completions"), &body, self.timeout)?;
        parse_top(v.pointer("/choices/0/logprobs/content/0/top_logprobs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ollama_shape() {
        let v = json!({"logprobs":[{"token":" B","logprob":-0.18,"top_logprobs":[{"token":" B","logprob":-0.18},{"token":" **","logprob":-1.9}]}]});
        let top = parse_top(v.pointer("/logprobs/0/top_logprobs")).unwrap();
        assert_eq!(top[0], (" B".to_owned(), -0.18));
        assert!(matches!(parse_top(None), Err(BackendError::NoLogprobs)));
    }

    #[test]
    fn chatml_primes_answer() {
        let p = OllamaRaw::chatml("sys", "q");
        assert!(p.ends_with("</think>\n\nAnswer:"));
    }
}
