use std::io::{self, IsTerminal};

use aghist::cli_error::ErrorEnvelope;

pub(super) fn should_emit_json(force_json: bool) -> bool {
    force_json || !io::stdout().is_terminal()
}

pub(super) fn llm_config_from_env(
    llm_model: Option<&str>,
) -> Result<aghist::llm::LlmConfig, ErrorEnvelope> {
    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config
            .with_model(model.to_string())
            .map_err(|e| map_llm_error(&e))?;
    }
    Ok(config)
}

pub(super) fn map_llm_error(e: &aghist::llm::LlmError) -> ErrorEnvelope {
    use aghist::llm::LlmError;
    let env = ErrorEnvelope::new("llm-error", e.to_string());
    match e {
        LlmError::MissingApiKey => {
            env.with_hint("Set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) and re-run.")
        }
        LlmError::ApiStatus {
            status: 401 | 403, ..
        } => env.with_hint("Verify ANTHROPIC_API_KEY is valid and has access to the chosen model."),
        LlmError::ApiStatus { status: 429, .. } => {
            env.with_hint("Rate limited — retry with --limit lowered or wait and retry.")
        }
        _ => env,
    }
}
