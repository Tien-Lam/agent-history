use std::env;
use std::time::Duration;

use super::LlmError;

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
    /// Cheap default - gives sensible quality on this task without paying for Sonnet.
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
