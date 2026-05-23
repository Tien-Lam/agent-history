use serde::Deserialize;

use super::super::common::{response_json_object, LlmError};
use super::StructuredTodo;

#[derive(Deserialize)]
struct TodosPayload {
    todos: Vec<StructuredTodo>,
}

/// Parse the Messages API response body into structured todos. Same
/// JSON-extraction tolerance as `parse_response`.
pub fn parse_todos_response(body: &str) -> Result<Vec<StructuredTodo>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: TodosPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "todos payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.todos)
}
