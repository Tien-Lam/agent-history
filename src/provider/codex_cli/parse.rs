use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::model::Message;
use crate::provider::json_text::string_or_object_field_or_pretty;
use crate::provider::ProviderError;

mod messages;
mod session;

pub(crate) use messages::parse_rollout_messages_with_stats;
pub(crate) use session::build_session_from_rollout;

pub(crate) fn parse_rollout_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    Ok(parse_rollout_messages_with_stats(path)?.messages)
}

fn entry_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["text", "content", "message", "output", "error"])
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: Option<Value>,
    content: Option<Value>,
    timestamp: Option<Value>,
    tool_calls: Option<serde_json::Value>,
    error: Option<Value>,
    /// Newer Codex format wraps messages in a payload object
    payload: Option<RawPayload>,
}

#[derive(Deserialize)]
struct RawPayload {
    #[serde(rename = "type")]
    entry_type: Option<Value>,
    /// `event_msg`: user/agent message text
    message: Option<Value>,
    /// `response_item` `function_call`: tool name
    name: Option<Value>,
    /// `response_item` `function_call`: call ID
    call_id: Option<Value>,
    /// `response_item` `function_call`: arguments as JSON string
    arguments: Option<Value>,
    /// `response_item` `function_call_output`: output text
    output: Option<Value>,
}
