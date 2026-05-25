use serde::Deserialize;

use super::super::common::{response_payload, LlmError};
use super::StructuredTodo;

#[derive(Deserialize)]
struct TodosPayload {
    todos: Vec<StructuredTodo>,
}

/// Parse the Messages API response body into structured todos. Same
/// JSON-extraction tolerance as `parse_response`.
pub fn parse_todos_response(body: &str) -> Result<Vec<StructuredTodo>, LlmError> {
    let parsed: TodosPayload = response_payload(body, "todos payload")?;
    Ok(parsed.todos)
}
