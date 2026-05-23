use std::env;
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

use crate::model::Role;

mod response;
mod transport;

#[cfg(test)]
pub(super) use response::extract_json_object;
pub(super) use response::response_json_object;
pub(super) use transport::post_request;
pub use transport::{LlmTransport, UreqTransport};

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
