use serde::Deserialize;

use super::super::common::{response_json_object, LlmError};
use super::StructuredThread;

#[derive(Deserialize)]
struct ThreadsPayload {
    threads: Vec<StructuredThread>,
}

/// Parse the Messages API response body into structured threads. Same
/// JSON-extraction rules as `parse_response`: tolerate code fences and
/// trailing prose.
pub fn parse_threads_response(body: &str) -> Result<Vec<StructuredThread>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: ThreadsPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "threads payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.threads)
}
