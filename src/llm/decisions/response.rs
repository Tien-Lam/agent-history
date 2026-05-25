use serde::Deserialize;

use super::super::common::{response_payload, LlmError};
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
    let parsed: DecisionsPayload = response_payload(body, "decisions payload")?;
    Ok(parsed.decisions)
}
