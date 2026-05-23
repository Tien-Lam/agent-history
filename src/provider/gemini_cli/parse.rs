use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::Role;
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{
    deserialize_optional_vec_skip_invalid, deserialize_vec_skip_invalid, timestamp_value_to_utc,
};

mod messages;
mod session;

pub(crate) use messages::load_messages_from_path_with_stats;
pub(crate) use session::{build_session_from_file, load_project_map};

fn raw_role(msg_type: Option<&Value>) -> Option<Role> {
    match stringish(msg_type, &["type", "role"]).as_deref() {
        Some("user") => Some(Role::User),
        Some("gemini") => Some(Role::Assistant),
        _ => None,
    }
}

fn text_parts(parts: &[TextPart]) -> String {
    parts
        .iter()
        .filter_map(|p| stringish(p.text.as_ref(), &["text", "content", "message"]))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Deserialize)]
struct RawSession {
    #[serde(rename = "sessionId")]
    session_id: Option<Value>,
    #[serde(rename = "startTime")]
    start_time: Option<Value>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_vec_skip_invalid")]
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "type")]
    msg_type: Option<Value>,
    #[serde(default)]
    content: RawContent,
    #[serde(rename = "displayContent")]
    #[serde(default, deserialize_with = "deserialize_optional_vec_skip_invalid")]
    display_content: Option<Vec<TextPart>>,
    #[serde(default, deserialize_with = "deserialize_optional_vec_skip_invalid")]
    thoughts: Option<Vec<Thought>>,
    tokens: Option<RawTokens>,
    #[serde(rename = "toolCalls")]
    #[serde(default, deserialize_with = "deserialize_optional_vec_skip_invalid")]
    tool_calls: Option<Vec<RawToolCall>>,
    model: Option<Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Text(String),
    Parts(Vec<TextPart>),
    Json(serde_json::Value),
}

impl Default for RawContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

#[derive(Deserialize)]
struct TextPart {
    text: Option<Value>,
}

#[derive(Deserialize)]
struct Thought {
    description: Option<Value>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<Value>,
    output: Option<Value>,
    cached: Option<Value>,
}

#[derive(Deserialize)]
struct RawToolCall {
    id: Option<Value>,
    name: Option<Value>,
    args: Option<serde_json::Value>,
    /// Populated by gemini-cli after the tool has executed. Shape varies -
    /// often `{"output": "..."}` or a free-form provider blob - so we
    /// accept any JSON value and stringify on read.
    response: Option<serde_json::Value>,
    /// Set when the tool execution failed.
    error: Option<serde_json::Value>,
}

fn gemini_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(value, &["timestamp", "startTime", "lastUpdated", "value"])
}
