use std::path::Path;

use chrono::{DateTime, Utc};

use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{string_or_pretty, stringish};
use crate::provider::parse_common::{
    pretty_json_opt, timestamp_value_to_utc, tool_result_block, tool_use_block, visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderError, ProviderMessageLoad, ProviderParseStats};

use super::{entry_text, RawEntry};

pub(crate) fn parse_rollout_messages_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Codex CLI messages");
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
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
                    skipped_records += 1;
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
        skipped_records,
        empty_content,
        messages = messages.len(),
        "Codex CLI message loading complete"
    );

    assign_fallback_message_ids(&mut messages);

    Ok(ProviderMessageLoad {
        messages,
        parse_stats: ProviderParseStats::from_counts(
            stats.line_count,
            stats.parse_errors,
            skipped_records,
            empty_content,
        ),
    })
}

fn entry_timestamp(entry: &RawEntry) -> DateTime<Utc> {
    timestamp_value_to_utc(entry.timestamp.as_ref(), &["timestamp", "time", "value"])
        .unwrap_or_else(Utc::now)
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
