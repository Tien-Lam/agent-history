use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::{
    string_or_object_field, string_or_object_field_or_pretty, string_or_pretty,
};
use crate::provider::parse_common::{
    parse_utc, parse_utc_or_now, pretty_json_opt, tool_result_block, tool_use_block,
    visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_session_from_rollout(path: &Path) -> Option<Session> {
    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut first_user_message: Option<String> = None;

    visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let entry = record.value;
            if let Some(dt) = timestamp_value_to_utc(entry.timestamp.as_ref()) {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }

            let entry_type = stringish(entry.entry_type.as_ref(), &["type"]);
            match entry_type.as_deref() {
                Some("user" | "assistant") => {
                    message_count += 1;
                    if entry_type.as_deref() == Some("user") && first_user_message.is_none() {
                        first_user_message = entry
                            .content
                            .as_ref()
                            .map(entry_text)
                            .map(|c| c.chars().take(80).collect());
                    }
                }
                Some("event_msg") => {
                    // Newer Codex format
                    if let Some(ref payload) = entry.payload {
                        let payload_type = stringish(payload.entry_type.as_ref(), &["type"]);
                        if let Some("user_message" | "agent_message") = payload_type.as_deref() {
                            message_count += 1;
                            if payload_type.as_deref() == Some("user_message")
                                && first_user_message.is_none()
                            {
                                first_user_message = payload
                                    .message
                                    .as_ref()
                                    .map(entry_text)
                                    .map(|m| m.chars().take(80).collect());
                            }
                        }
                    }
                }
                _ => {}
            }
        },
        |_| {},
    )
    .ok()?;

    if message_count == 0 {
        return None;
    }

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::CodexCli,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at: first_timestamp?,
        ended_at: last_timestamp,
        summary: first_user_message,
        model: None,
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    })
}

pub(crate) fn parse_rollout_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Codex CLI messages");
    let mut messages = Vec::new();
    let mut skipped_types: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let line_number = record.line_number;
            let entry = record.value;
            let entry_type = stringish(entry.entry_type.as_ref(), &["type"]).unwrap_or_default();
            let role = match entry_type.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool_use" => Role::Tool,
                "error" => {
                    if let Some(error_msg) = entry.error.as_ref().map(entry_text) {
                        messages.push(error_message(entry_timestamp(&entry), error_msg));
                    }
                    return;
                }
                "event_msg" => {
                    push_event_msg(&mut messages, &entry);
                    return;
                }
                "response_item" => {
                    push_response_item(&mut messages, &entry);
                    return;
                }
                _ => {
                    skipped_types += 1;
                    tracing::trace!(
                        entry_type = entry_type.as_str(),
                        "skipping non-message entry"
                    );
                    return;
                }
            };

            let timestamp = entry_timestamp(&entry);
            let content = legacy_content(&entry, role);

            if content.is_empty() {
                empty_content += 1;
                tracing::trace!(
                    line_num = line_number,
                    entry_type = entry_type.as_str(),
                    "skipping entry with empty content"
                );
                return;
            }

            messages.push(message(role, timestamp, content));
        },
        |error| {
            tracing::warn!(line_num = error.line_number, error = %error.error, "failed to parse JSONL line");
        },
    )?;

    tracing::info!(
        path = %path.display(),
        lines = stats.line_count,
        parse_errors = stats.parse_errors,
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Codex CLI message loading complete"
    );

    assign_fallback_message_ids(&mut messages);

    Ok(messages)
}

fn entry_timestamp(entry: &RawEntry) -> DateTime<Utc> {
    let timestamp = stringish(entry.timestamp.as_ref(), &["timestamp", "time"]);
    parse_utc_or_now(timestamp.as_deref())
}

fn message(role: Role, timestamp: DateTime<Utc>, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(String::new()),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    }
}

fn assign_fallback_message_ids(messages: &mut [Message]) {
    for (idx, message) in messages.iter_mut().enumerate() {
        if message.id.0.is_empty() {
            message.id = MessageId(format!("codex-turn-{}", idx + 1));
        }
    }
}

fn error_message(timestamp: DateTime<Utc>, error_msg: String) -> Message {
    message(
        Role::System,
        timestamp,
        vec![ContentBlock::Error(error_msg)],
    )
}

fn push_text_message(
    messages: &mut Vec<Message>,
    role: Role,
    timestamp: DateTime<Utc>,
    text: &str,
) {
    if !text.is_empty() {
        messages.push(message(role, timestamp, parse_text_with_code_blocks(text)));
    }
}

fn push_event_msg(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = stringish(payload.entry_type.as_ref(), &["type"]).unwrap_or_default();
    let timestamp = entry_timestamp(entry);
    match payload_type.as_str() {
        "user_message" => {
            if let Some(msg_text) = payload.message.as_ref().map(entry_text) {
                push_text_message(messages, Role::User, timestamp, &msg_text);
            }
        }
        "agent_message" => {
            if let Some(msg_text) = payload.message.as_ref().map(entry_text) {
                push_text_message(messages, Role::Assistant, timestamp, &msg_text);
            }
        }
        _ => tracing::trace!(payload_type = payload_type.as_str(), "skipping event_msg"),
    }
}

fn push_response_item(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = stringish(payload.entry_type.as_ref(), &["type"]).unwrap_or_default();
    let timestamp = entry_timestamp(entry);
    match payload_type.as_str() {
        "function_call" => messages.push(message(
            Role::Tool,
            timestamp,
            vec![tool_use_block(
                payload
                    .call_id
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["call_id", "id"]))
                    .unwrap_or_default(),
                payload
                    .name
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["name", "tool"]))
                    .unwrap_or_else(|| "unknown".to_string()),
                payload
                    .arguments
                    .as_ref()
                    .map(string_or_pretty)
                    .unwrap_or_default(),
            )],
        )),
        "function_call_output" => {
            let output = payload.output.as_ref().map(entry_text).unwrap_or_default();
            if !output.is_empty() {
                messages.push(message(
                    Role::Tool,
                    timestamp,
                    vec![tool_result_block(
                        payload
                            .call_id
                            .as_ref()
                            .and_then(|value| stringish(Some(value), &["call_id", "id"]))
                            .unwrap_or_default(),
                        true,
                        output,
                    )],
                ));
            }
        }
        _ => tracing::trace!(
            payload_type = payload_type.as_str(),
            "skipping response_item"
        ),
    }
}

fn legacy_content(entry: &RawEntry, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    let Some(text) = entry
        .content
        .as_ref()
        .map(entry_text)
        .filter(|text| !text.is_empty())
    else {
        return content;
    };
    if role == Role::Tool {
        content.push(tool_use_block(
            String::new(),
            text.clone(),
            pretty_json_opt(entry.tool_calls.as_ref()),
        ));
    } else {
        content.extend(parse_text_with_code_blocks(&text));
    }
    content
}

fn entry_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["text", "content", "message", "output", "error"])
}

fn stringish(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let value = value?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(map) => object_fields
            .iter()
            .find_map(|field| stringish(map.get(*field), object_fields))
            .or_else(|| {
                let text = string_or_object_field(value, object_fields);
                (!text.is_empty()).then_some(text)
            }),
        _ => None,
    }
}

fn timestamp_value_to_utc(value: Option<&Value>) -> Option<DateTime<Utc>> {
    match value? {
        Value::String(text) => parse_utc(text),
        Value::Object(map) => ["timestamp", "time", "value"]
            .iter()
            .find_map(|field| timestamp_value_to_utc(map.get(*field))),
        _ => None,
    }
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
