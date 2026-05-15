use std::env;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Role;

/// Errors surfaced from the LLM extraction path. Mapped to the
/// `llm-error` envelope kind at the CLI boundary.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("missing API key: set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) before running --llm")]
    MissingApiKey,
    #[error("HTTP request to {url} failed: {source}")]
    Http {
        url: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("API at {url} returned status {status}: {body}")]
    ApiStatus {
        url: String,
        status: u16,
        body: String,
    },
    #[error("could not parse LLM response: {0}")]
    Parse(String),
    #[error("model returned no parsable JSON in its reply: {0}")]
    NoJson(String),
}

/// Runtime configuration for the LLM extractor. Constructed via
/// [`LlmConfig::from_env`] in normal use; tests build it directly.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub anthropic_version: String,
    pub timeout: Duration,
}

impl LlmConfig {
    /// Cheap default — gives sensible quality on this task without paying for Sonnet.
    pub const DEFAULT_MODEL: &'static str = "claude-haiku-4-5-20251001";
    pub const DEFAULT_ENDPOINT: &'static str = "https://api.anthropic.com/v1/messages";
    pub const DEFAULT_VERSION: &'static str = "2023-06-01";
    pub const DEFAULT_MAX_TOKENS: u32 = 1024;
    pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

    /// Build config from process environment. `AGHIST_LLM_API_KEY` wins over
    /// `ANTHROPIC_API_KEY` so users can scope a separate key per tool.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = env::var("AGHIST_LLM_API_KEY")
            .ok()
            .or_else(|| env::var("ANTHROPIC_API_KEY").ok())
            .filter(|s| !s.is_empty())
            .ok_or(LlmError::MissingApiKey)?;
        let endpoint = env::var("AGHIST_LLM_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_ENDPOINT.to_string());
        let model = env::var("AGHIST_LLM_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let anthropic_version = env::var("AGHIST_LLM_ANTHROPIC_VERSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_VERSION.to_string());
        Ok(Self {
            endpoint,
            api_key,
            model,
            max_tokens: Self::DEFAULT_MAX_TOKENS,
            anthropic_version,
            timeout: Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS),
        })
    }

    /// Override the model (e.g. from the CLI flag).
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }
}

/// HTTP transport boundary. Production uses [`UreqTransport`]; tests inject
/// a mock so the extractor can be exercised without a network.
pub trait LlmTransport: Send + Sync {
    /// POST `body` (JSON) to `url` with the supplied headers, returning
    /// `(status, body)`. Implementations MUST NOT raise on non-2xx — let
    /// the caller decide based on status.
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError>;
}

/// `ureq`-backed transport. Synchronous to fit the rest of aghist's IO model.
pub struct UreqTransport {
    timeout: Duration,
}

impl UreqTransport {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new(Duration::from_secs(LlmConfig::DEFAULT_TIMEOUT_SECS))
    }
}

impl LlmTransport for UreqTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError> {
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let mut req = agent.post(url).set("content-type", "application/json");
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_string(body) {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.into_string().map_err(|e| LlmError::Http {
                    url: url.to_string(),
                    source: Box::new(e),
                })?;
                Ok((status, text))
            }
            Err(ureq::Error::Status(status, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                Ok((status, text))
            }
            Err(e) => Err(LlmError::Http {
                url: url.to_string(),
                source: Box::new(e),
            }),
        }
    }
}

pub(super) fn role_label(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    cache_control: CacheControl,
}

#[derive(Serialize)]
struct UserMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    system: [SystemBlock<'a>; 1],
    messages: [UserMessage<'a>; 1],
}

/// Build the JSON request body for an extraction call with a caller-supplied
/// system prompt. All LLM routes share this shape so prompt caching behaves
/// consistently.
pub(super) fn build_request_body_with_system(
    config: &LlmConfig,
    system: &str,
    user: &str,
) -> Result<String, LlmError> {
    let req = Request {
        model: &config.model,
        max_tokens: config.max_tokens,
        system: [SystemBlock {
            kind: "text",
            text: system,
            cache_control: CacheControl { kind: "ephemeral" },
        }],
        messages: [UserMessage {
            role: "user",
            content: user,
        }],
    };
    serde_json::to_string(&req).map_err(|e| LlmError::Parse(e.to_string()))
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    content: Vec<ApiContentBlock>,
}

#[derive(Deserialize)]
struct ApiContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

fn request_headers(config: &LlmConfig) -> [(&str, &str); 3] {
    [
        ("x-api-key", config.api_key.as_str()),
        ("anthropic-version", config.anthropic_version.as_str()),
        ("content-type", "application/json"),
    ]
}

pub(super) fn post_request<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    body: &str,
) -> Result<String, LlmError> {
    let headers = request_headers(config);
    let (status, resp_body) = transport.post_json(&config.endpoint, &headers, body)?;
    if !(200..300).contains(&status) {
        return Err(LlmError::ApiStatus {
            url: config.endpoint.clone(),
            status,
            body: resp_body.chars().take(500).collect(),
        });
    }
    Ok(resp_body)
}

pub(super) fn response_json_object(body: &str) -> Result<String, LlmError> {
    let resp: ApiResponse = serde_json::from_str(body)
        .map_err(|e| LlmError::Parse(format!("response envelope: {e}")))?;
    let text = resp
        .content
        .into_iter()
        .find(|b| b.kind == "text")
        .map(|b| b.text)
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(LlmError::NoJson("empty assistant text".into()));
    }
    extract_json_object(&text)
        .map(str::to_string)
        .ok_or_else(|| LlmError::NoJson(text.chars().take(200).collect()))
}

/// Find the first balanced `{...}` object in `text`. Returns `None` if no
/// balanced object is present. Skips over braces that appear inside double-
/// quoted strings (with `\\` escape handling) so JSON-with-prose still parses.
pub(super) fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}
