use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{
    string_or_object_field_or_pretty, string_or_typed_text_array_or_pretty, stringish, value_bool,
    value_u64,
};
use crate::provider::parse_common::{
    pretty_json_opt, timestamp_value_to_utc, token_usage_from_options, tool_result_block,
    tool_use_block, visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

mod session;

pub(crate) use session::{build_session_metadata, decode_project_name, parse_history_index};

#[derive(Deserialize)]
struct RawSessionEntry {
    #[serde(rename = "type")]
    entry_type: Option<Value>,
    uuid: Option<Value>,
    timestamp: Option<Value>,
    message: Option<RawMessage>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<Value>,
    cwd: Option<Value>,
}

#[derive(Deserialize)]
struct RawMessage {
    content: Option<serde_json::Value>,
    model: Option<Value>,
    usage: Option<RawUsage>,
}

#[allow(clippy::struct_field_names)] // Provider JSON uses token-suffixed usage fields.
#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<Value>,
    output_tokens: Option<Value>,
    cache_read_input_tokens: Option<Value>,
    cache_creation_input_tokens: Option<Value>,
}

pub(crate) fn parse_session_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    Ok(parse_session_messages_with_stats(path)?.messages)
}

pub(crate) fn parse_session_messages_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Claude Code messages");
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawSessionEntry, _, _>(
        path,
        |record| {
            let line_number = record.line_number;
            let entry = record.value;
            let role_text = stringish(entry.entry_type.as_ref(), &["type"]);
            let role = match role_text.as_deref() {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                Some(other) => {
                    skipped_records += 1;
                    tracing::trace!(entry_type = other, "skipping non-message entry");
                    return;
                }
                None => {
                    skipped_records += 1;
                    return;
                }
            };

            let Some(ref msg) = entry.message else {
                skipped_records += 1;
                tracing::warn!(line_num = line_number, role = ?role, uuid = ?entry.uuid, "entry has no message field");
                return;
            };

            let timestamp = claude_timestamp(entry.timestamp.as_ref()).unwrap_or_else(Utc::now);

            let id = stringish(entry.uuid.as_ref(), &["uuid", "id"]).unwrap_or_default();

            let content = parse_message_content(msg, role);
            if content.is_empty() {
                empty_content += 1;
                tracing::trace!(line_num = line_number, msg_id = %id, role = ?role, "skipping message with empty content");
                return;
            }

            let token_usage = msg.usage.as_ref().map(|u| {
                token_usage_from_options(
                    value_u64(u.input_tokens.as_ref()),
                    value_u64(u.output_tokens.as_ref()),
                    value_u64(u.cache_read_input_tokens.as_ref()),
                    value_u64(u.cache_creation_input_tokens.as_ref()),
                )
            });

            messages.push(Message {
                id: MessageId(id),
                role,
                timestamp,
                content,
                model: stringish(msg.model.as_ref(), &["model", "id", "name"]),
                token_usage,
            });
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
        "Claude Code message loading complete"
    );

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

fn parse_message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let Some(content) = &msg.content else {
        return Vec::new();
    };

    match content {
        serde_json::Value::String(s) => parse_text_with_code_blocks(s),
        serde_json::Value::Array(arr) => {
            let mut blocks = Vec::new();
            for item in arr {
                let item_type = stringish(item.get("type"), &["type"]).unwrap_or_default();
                match item_type.as_str() {
                    "text" => {
                        if let Some(text) = item
                            .get("text")
                            .map(|v| string_or_object_field_or_pretty(v, &["text", "content"]))
                            .filter(|text| !text.is_empty())
                        {
                            blocks.extend(parse_text_with_code_blocks(&text));
                        }
                    }
                    "thinking" => {
                        if let Some(text) = item
                            .get("thinking")
                            .map(|v| string_or_object_field_or_pretty(v, &["thinking", "text"]))
                            .filter(|text| !text.is_empty())
                        {
                            blocks.push(ContentBlock::Thinking(text));
                        }
                    }
                    "tool_use" => {
                        let name = stringish(item.get("name"), &["name", "tool", "toolName"])
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| "unknown".to_string());
                        let id = stringish(item.get("id"), &["id"]).unwrap_or_default();
                        let arguments = pretty_json_opt(item.get("input"));
                        blocks.push(tool_use_block(id, name, arguments));
                    }
                    "tool_result" if role == Role::User => {
                        let tool_call_id =
                            stringish(item.get("tool_use_id"), &["tool_use_id", "toolUseId", "id"])
                                .unwrap_or_default();
                        let is_error =
                            value_bool(item.get("is_error"), &["is_error", "isError", "value"])
                                .unwrap_or(false);
                        let output = extract_tool_result_text(item);
                        blocks.push(tool_result_block(tool_call_id, !is_error, output));
                    }
                    _ => {}
                }
            }
            blocks
        }
        serde_json::Value::Object(_) => {
            let text = string_or_object_field_or_pretty(content, &["text", "content", "message"]);
            if text.is_empty() {
                Vec::new()
            } else {
                parse_text_with_code_blocks(&text)
            }
        }
        _ => Vec::new(),
    }
}

fn extract_tool_result_text(item: &serde_json::Value) -> String {
    item.get("content")
        .map(|content| match content {
            Value::Object(_) => {
                string_or_object_field_or_pretty(content, &["text", "content", "message", "output"])
            }
            _ => string_or_typed_text_array_or_pretty(content, "text", "text"),
        })
        .unwrap_or_default()
}

fn claude_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(value, &["timestamp", "createdAt", "value"])
}

#[cfg(test)]
mod tests;
