use serde::Deserialize;

use super::super::common::{response_json_object, LlmError};
use super::StructuredDecision;

#[derive(Deserialize)]
struct DecisionsPayload {
    decisions: Vec<StructuredDecision>,
}

/// Parse the Messages API response body into structured decisions.
///
/// The model is asked for JSON only, but real-world models occasionally
/// wrap output in ` ```json ... ``` ` fences or add a trailing prose
/// sentence. We extract the first JSON object from the assistant text and
/// parse that, so small formatting drift doesn't break the pipeline.
pub fn parse_response(body: &str) -> Result<Vec<StructuredDecision>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: DecisionsPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "decisions payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.decisions)
}
