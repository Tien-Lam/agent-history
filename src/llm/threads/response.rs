use serde::Deserialize;

use super::super::common::{response_payload, LlmError};
use super::StructuredThread;

#[derive(Deserialize)]
struct ThreadsPayload {
    threads: Vec<StructuredThread>,
}

/// Parse the Messages API response body into structured threads. Same
/// JSON-extraction rules as `parse_response`: tolerate code fences and
/// trailing prose.
pub fn parse_threads_response(body: &str) -> Result<Vec<StructuredThread>, LlmError> {
    let parsed: ThreadsPayload = response_payload(body, "threads payload")?;
    Ok(parsed.threads)
}
