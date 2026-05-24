use chrono::{DateTime, Utc};

use crate::model::{ContentBlock, Message, Role};
use crate::provider::json_text::{string_or_pretty, stringish};
use crate::provider::parse_common::{pretty_json_opt, tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

use super::super::{entry_text, RawEntry};
use super::util::{entry_timestamp, message};

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

pub(super) fn push_event_msg(messages: &mut Vec<Message>, entry: &RawEntry, fallback_idx: usize) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = stringish(payload.entry_type.as_ref(), &["type"]).unwrap_or_default();
    let timestamp = entry_timestamp(entry, fallback_idx);
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

pub(super) fn push_response_item(
    messages: &mut Vec<Message>,
    entry: &RawEntry,
    fallback_idx: usize,
) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = stringish(payload.entry_type.as_ref(), &["type"]).unwrap_or_default();
    let timestamp = entry_timestamp(entry, fallback_idx);
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

pub(super) fn legacy_content(entry: &RawEntry, role: Role) -> Vec<ContentBlock> {
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
