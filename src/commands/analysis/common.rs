use aghist::cli_error::ErrorEnvelope;

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
