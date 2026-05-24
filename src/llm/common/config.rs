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
        let api_key = llm_api_key_from_env_values(
            env::var("AGHIST_LLM_API_KEY").ok(),
            env::var("ANTHROPIC_API_KEY").ok(),
        )
        .ok_or(LlmError::MissingApiKey)?;
        let endpoint = env::var("AGHIST_LLM_ENDPOINT")
            .ok()
            .and_then(non_blank_env_string)
            .unwrap_or_else(|| Self::DEFAULT_ENDPOINT.to_string());
        let model = env::var("AGHIST_LLM_MODEL")
            .ok()
            .and_then(non_blank_env_string)
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let anthropic_version = env::var("AGHIST_LLM_ANTHROPIC_VERSION")
            .ok()
            .and_then(non_blank_env_string)
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

fn non_blank_env_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn llm_api_key_from_env_values(
    primary: Option<String>,
    fallback: Option<String>,
) -> Option<String> {
    primary
        .and_then(non_blank_env_string)
        .or_else(|| fallback.and_then(non_blank_env_string))
}

#[cfg(test)]
mod tests {
    use super::{llm_api_key_from_env_values, non_blank_env_string};

    #[test]
    fn non_blank_env_string_ignores_blank_values() {
        assert_eq!(non_blank_env_string(String::new()), None);
        assert_eq!(non_blank_env_string(" \t ".to_string()), None);
        assert_eq!(
            non_blank_env_string("value with spaces".to_string()),
            Some("value with spaces".to_string())
        );
    }

    #[test]
    fn blank_primary_api_key_falls_back_to_anthropic_key() {
        assert_eq!(
            llm_api_key_from_env_values(Some(" \t ".to_string()), Some("fallback".to_string())),
            Some("fallback".to_string())
        );
    }
}
