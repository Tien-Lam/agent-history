use std::env;
use std::time::Duration;

use crate::schema_fragments::{
    LLM_API_KEY_MAX_BYTES, LLM_ENDPOINT_MAX_BYTES, LLM_MODEL_MAX_BYTES, LLM_VERSION_MAX_BYTES,
};

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
        )?
        .ok_or(LlmError::MissingApiKey)?;
        let endpoint = env::var("AGHIST_LLM_ENDPOINT")
            .ok()
            .map(|value| non_blank_env_string("AGHIST_LLM_ENDPOINT", value, LLM_ENDPOINT_MAX_BYTES))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| Self::DEFAULT_ENDPOINT.to_string());
        let model = env::var("AGHIST_LLM_MODEL")
            .ok()
            .map(|value| non_blank_env_string("AGHIST_LLM_MODEL", value, LLM_MODEL_MAX_BYTES))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let anthropic_version = env::var("AGHIST_LLM_ANTHROPIC_VERSION")
            .ok()
            .map(|value| {
                non_blank_env_string("AGHIST_LLM_ANTHROPIC_VERSION", value, LLM_VERSION_MAX_BYTES)
            })
            .transpose()?
            .flatten()
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
    pub fn with_model(mut self, model: String) -> Result<Self, LlmError> {
        enforce_env_string_limit("llm_model", &model, LLM_MODEL_MAX_BYTES)?;
        self.model = model;
        Ok(self)
    }
}

fn non_blank_env_string(
    name: &'static str,
    value: String,
    max_bytes: usize,
) -> Result<Option<String>, LlmError> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        enforce_env_string_limit(name, &value, max_bytes)?;
        Ok(Some(value))
    }
}

fn llm_api_key_from_env_values(
    primary: Option<String>,
    fallback: Option<String>,
) -> Result<Option<String>, LlmError> {
    if let Some(value) = primary {
        if let Some(value) =
            non_blank_env_string("AGHIST_LLM_API_KEY", value, LLM_API_KEY_MAX_BYTES)?
        {
            return Ok(Some(value));
        }
    }
    if let Some(value) = fallback {
        return non_blank_env_string("ANTHROPIC_API_KEY", value, LLM_API_KEY_MAX_BYTES);
    }
    Ok(None)
}

fn enforce_env_string_limit(
    name: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), LlmError> {
    let bytes = value.len();
    if bytes > max_bytes {
        Err(LlmError::ConfigValueTooLarge {
            name,
            bytes,
            max_bytes,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{llm_api_key_from_env_values, non_blank_env_string};
    use crate::{llm::LlmError, schema_fragments::LLM_MODEL_MAX_BYTES};

    #[test]
    fn non_blank_env_string_ignores_blank_values() {
        assert_eq!(
            non_blank_env_string("TEST", String::new(), LLM_MODEL_MAX_BYTES).unwrap(),
            None
        );
        assert_eq!(
            non_blank_env_string("TEST", " \t ".to_string(), LLM_MODEL_MAX_BYTES).unwrap(),
            None
        );
        assert_eq!(
            non_blank_env_string("TEST", "value with spaces".to_string(), LLM_MODEL_MAX_BYTES)
                .unwrap(),
            Some("value with spaces".to_string())
        );
    }

    #[test]
    fn blank_primary_api_key_falls_back_to_anthropic_key() {
        assert_eq!(
            llm_api_key_from_env_values(Some(" \t ".to_string()), Some("fallback".to_string()))
                .unwrap(),
            Some("fallback".to_string())
        );
    }

    #[test]
    fn env_string_limits_reject_oversized_values() {
        let oversized = "x".repeat(LLM_MODEL_MAX_BYTES + 1);
        let err =
            non_blank_env_string("AGHIST_LLM_MODEL", oversized, LLM_MODEL_MAX_BYTES).unwrap_err();

        assert!(matches!(
            err,
            LlmError::ConfigValueTooLarge {
                name: "AGHIST_LLM_MODEL",
                ..
            }
        ));
    }
}
