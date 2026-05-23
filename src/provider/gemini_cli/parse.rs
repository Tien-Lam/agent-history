use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_u64};
use crate::provider::parse_common::{
    deserialize_optional_vec_skip_invalid, deserialize_vec_skip_invalid, pretty_json_opt,
    timestamp_value_to_utc, token_usage_from_options, tool_result_block, tool_use_block,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

mod session;

pub(crate) use session::{build_session_from_file, load_project_map};

pub(crate) fn load_messages_from_path_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Gemini CLI messages");
    let data = std::fs::read_to_string(path)?;
    let raw: Value = serde_json::from_str(&data)?;
    let raw_messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (messages, parse_stats) = convert_message_values(raw_messages);
    tracing::info!(
        path = %path.display(),
        raw_messages = parse_stats.records_seen,
        parsed_messages = messages.len(),
        parse_errors = parse_stats.parse_errors,
        skipped_records = parse_stats.skipped_records,
        empty_content = parse_stats.empty_content,
        "Gemini CLI message loading complete"
    );
    Ok(ProviderMessageLoad {
        messages,
        parse_stats,
    })
}

fn convert_message_values(raw_messages: Vec<Value>) -> (Vec<Message>, ProviderParseStats) {
    let mut messages = Vec::with_capacity(raw_messages.len());
    let mut parse_stats = ProviderParseStats::default();

    for raw in raw_messages {
        parse_stats.record_seen();
        let Ok(msg) = serde_json::from_value::<RawMessage>(raw) else {
            parse_stats.record_parse_error();
            continue;
        };

        let Some(role) = raw_role(msg.msg_type.as_ref()) else {
            parse_stats.record_skipped_record();
            continue;
        };

        let content = message_content(&msg, role);
        if content.is_empty() {
            parse_stats.record_empty_content();
            continue;
        }

        messages.push(message_from_content(&msg, role, content));
    }

    (messages, parse_stats)
}

fn message_from_content(msg: &RawMessage, role: Role, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(stringish(msg.id.as_ref(), &["id"]).unwrap_or_default()),
        role,
        timestamp: message_timestamp(msg.timestamp.as_ref()),
        content,
        model: stringish(msg.model.as_ref(), &["model", "id", "name"]),
        token_usage: msg.tokens.as_ref().map(|tokens| {
            token_usage_from_options(
                value_u64(tokens.input.as_ref()),
                value_u64(tokens.output.as_ref()),
                value_u64(tokens.cached.as_ref()),
                None,
            )
        }),
    }
}

fn raw_role(msg_type: Option<&Value>) -> Option<Role> {
    match stringish(msg_type, &["type", "role"]).as_deref() {
        Some("user") => Some(Role::User),
        Some("gemini") => Some(Role::Assistant),
        _ => None,
    }
}

fn message_timestamp(raw: Option<&Value>) -> DateTime<Utc> {
    gemini_timestamp(raw).unwrap_or_else(Utc::now)
}

fn message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();

    let text = message_text(msg, role);
    if !text.is_empty() {
        content.extend(parse_text_with_code_blocks(&text));
    }

    append_thoughts(&mut content, msg.thoughts.as_deref());
    append_tool_calls(&mut content, msg.tool_calls.as_deref());

    content
}

fn message_text(msg: &RawMessage, role: Role) -> String {
    match &msg.content {
        RawContent::Text(s) => s.clone(),
        RawContent::Parts(parts) if role == Role::User => {
            let preferred = msg.display_content.as_deref().unwrap_or(parts);
            text_parts(preferred)
        }
        RawContent::Parts(parts) => text_parts(parts),
        RawContent::Json(value) => {
            string_or_object_field_or_pretty(value, &["text", "content", "message"])
        }
    }
}

fn text_parts(parts: &[TextPart]) -> String {
    parts
        .iter()
        .filter_map(|p| stringish(p.text.as_ref(), &["text", "content", "message"]))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_thoughts(content: &mut Vec<ContentBlock>, thoughts: Option<&[Thought]>) {
    let Some(thoughts) = thoughts else {
        return;
    };

    for thought in thoughts {
        if let Some(desc) = stringish(
            thought.description.as_ref(),
            &["description", "text", "content"],
        )
        .filter(|desc| !desc.is_empty())
        {
            content.push(ContentBlock::Thinking(desc));
        }
    }
}

fn append_tool_calls(content: &mut Vec<ContentBlock>, tool_calls: Option<&[RawToolCall]>) {
    let Some(tool_calls) = tool_calls else {
        return;
    };

    for tc in tool_calls {
        let id = stringish(tc.id.as_ref(), &["id", "toolCallId"]).unwrap_or_default();
        content.push(tool_use_block(
            id.clone(),
            stringish(tc.name.as_ref(), &["name", "toolName"])
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(tc.args.as_ref()),
        ));

        append_tool_response(content, tc, id);
    }
}

fn append_tool_response(content: &mut Vec<ContentBlock>, tc: &RawToolCall, id: String) {
    let Some(output) = tc.response.as_ref().map(extract_tool_response_text) else {
        return;
    };

    if !output.is_empty() {
        content.push(tool_result_block(id, tc.error.is_none(), output));
    }
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

fn extract_tool_response_text(v: &serde_json::Value) -> String {
    string_or_object_field_or_pretty(v, &["output", "result", "content", "text"])
}
