use serde::Deserialize;
use serde_json::Value;

use crate::provider::json_text::string_or_object_field_or_pretty;
use crate::provider::parse_common::timestamp_value_to_utc;

mod messages;
mod session;

pub(crate) use messages::{message_id_from_file, parse_message_file_with_stats};
pub(crate) use session::build_session_from_file;

fn message_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["text", "content", "message", "title", "diff"])
}

fn tool_output_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["output", "result", "content", "text"])
}

fn timestamp_from_values(
    millis: Option<&Value>,
    raw: Option<&Value>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    opencode_timestamp(millis).or_else(|| opencode_timestamp(raw))
}

fn opencode_timestamp(value: Option<&Value>) -> Option<chrono::DateTime<chrono::Utc>> {
    timestamp_value_to_utc(value, &["created", "updated", "timestamp", "value"])
}

#[derive(Deserialize)]
struct RawTime {
    created: Option<Value>,
    updated: Option<Value>,
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(rename = "modelID")]
    model_id: Option<Value>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<Value>,
    role: Option<Value>,
    /// Legacy format: ISO timestamp
    timestamp: Option<Value>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: text content
    content: Option<Value>,
    /// Legacy format: code changes
    #[serde(rename = "codeChanges")]
    code_changes: Option<Vec<RawCodeChange>>,
    /// New format: summary with title and diffs
    summary: Option<RawSummary>,
    /// New format: token usage
    tokens: Option<RawTokens>,
    /// New format: model info
    model: Option<RawModel>,
}

#[derive(Deserialize)]
struct RawSummary {
    title: Option<Value>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<Value>,
    output: Option<Value>,
    cache: Option<RawCache>,
}

#[derive(Deserialize)]
struct RawCache {
    read: Option<Value>,
    write: Option<Value>,
}

#[derive(Deserialize)]
struct RawCodeChange {
    path: Option<Value>,
    diff: Option<Value>,
}

#[derive(Deserialize)]
struct RawPart {
    #[serde(rename = "type")]
    part_type: Option<Value>,
    /// Text content (for type="text")
    text: Option<Value>,
    /// Tool name (for type="tool")
    tool: Option<Value>,
    /// Tool call ID (for type="tool")
    #[serde(rename = "callID")]
    call_id: Option<Value>,
    /// Tool state with input/output (for type="tool")
    state: Option<RawToolState>,
}

#[derive(Deserialize)]
struct RawToolState {
    status: Option<Value>,
    input: Option<serde_json::Value>,
    output: Option<Value>,
}
