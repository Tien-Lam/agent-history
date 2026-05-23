use thiserror::Error;

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
