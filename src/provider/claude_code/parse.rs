use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::{
    string_or_object_field_or_pretty, string_or_typed_text_array_or_pretty, stringish, value_bool,
    value_i64, value_u64,
};
use crate::provider::parse_common::{
    millis_to_utc, nonzero_token_usage, parse_jsonl_records, parse_utc, pretty_json_opt,
    token_usage_from_options, tool_result_block, tool_use_block, visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Deserialize)]
pub(crate) struct HistoryEntry {
    display: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "sessionId")]
    session_id: Option<Value>,
}

pub(crate) fn parse_history_index(path: &Path) -> Result<Vec<HistoryEntry>, ProviderError> {
    Ok(parse_jsonl_records::<HistoryEntry>(path)?
        .records
        .into_iter()
        .map(|record| record.value)
        .collect())
}

/// Decode the project directory name back to a readable path.
/// Claude Code encodes `V:\Projects\agent-history` as `V--Projects-agent-history`.
/// The encoding is lossy (both `/` and literal `-` become `-`), so we only
/// decode `--` (drive separator) and leave single dashes as-is.
pub(crate) fn decode_project_name(encoded: &str) -> String {
    encoded.replace("--", ":/")
}

pub(crate) fn build_session_metadata(
    source_path: &Path,
    session_id: &str,
    project_name: &str,
    history_entries: &[HistoryEntry],
) -> Option<Session> {
    // Quick scan of the session file for timestamps and message count
    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    visit_jsonl_records::<RawSessionEntry, _, _>(
        source_path,
        |record| {
            let entry = record.value;
            if let Some(dt) = timestamp_value_to_utc(entry.timestamp.as_ref()) {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }

            let entry_role = stringish(entry.entry_type.as_ref(), &["type"]);
            if let Some("user" | "assistant") = entry_role.as_deref() {
                message_count += 1;

                if git_branch.is_none() {
                    if let Some(branch) =
                        stringish(entry.git_branch.as_ref(), &["gitBranch", "branch"])
                    {
                        git_branch = Some(branch);
                    }
                }
                if cwd.is_none() {
                    if let Some(c) = stringish(entry.cwd.as_ref(), &["cwd", "path"]) {
                        cwd = Some(c);
                    }
                }

                if entry_role.as_deref() == Some("assistant") {
                    if let Some(ref msg) = entry.message {
                        if model.is_none() {
                            if let Some(m) = stringish(msg.model.as_ref(), &["model", "id", "name"])
                            {
                                model = Some(m);
                            }
                        }
                        if let Some(ref usage) = msg.usage {
                            total_input_tokens +=
                                value_u64(usage.input_tokens.as_ref()).unwrap_or(0);
                            total_output_tokens +=
                                value_u64(usage.output_tokens.as_ref()).unwrap_or(0);
                        }
                    }
                }
            }
        },
        |_| {},
    )
    .ok()?;

    if message_count == 0 {
        return None;
    }

    // Get first user message as summary from history entries
    let summary = history_entries
        .iter()
        .find(|e| {
            stringish(e.session_id.as_ref(), &["sessionId", "id"]).as_deref() == Some(session_id)
        })
        .and_then(|e| stringish(e.display.as_ref(), &["display", "text", "content"]));

    // Use history entry timestamp if we didn't find one in the session file
    let started_at = first_timestamp.or_else(|| {
        history_entries
            .iter()
            .find(|e| {
                stringish(e.session_id.as_ref(), &["sessionId", "id"]).as_deref()
                    == Some(session_id)
            })
            .and_then(|e| timestamp_value_to_utc(e.timestamp.as_ref()))
    })?;

    let token_usage = nonzero_token_usage(total_input_tokens, total_output_tokens, None, None);

    Some(Session {
        id: SessionId(session_id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: cwd.map(PathBuf::from),
        project_name: Some(project_name.to_string()),
        git_branch,
        started_at,
        ended_at: last_timestamp,
        summary,
        model,
        token_usage,
        message_count,
        source_path: source_path.to_path_buf(),
    })
}

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
    tracing::debug!(path = %path.display(), "loading Claude Code messages");
    let mut messages = Vec::new();
    let mut skipped_types: usize = 0;
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
                    skipped_types += 1;
                    tracing::trace!(entry_type = other, "skipping non-message entry");
                    return;
                }
                None => {
                    skipped_types += 1;
                    return;
                }
            };

            let Some(ref msg) = entry.message else {
                tracing::warn!(line_num = line_number, role = ?role, uuid = ?entry.uuid, "entry has no message field");
                return;
            };

            let timestamp =
                timestamp_value_to_utc(entry.timestamp.as_ref()).unwrap_or_else(Utc::now);

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
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Claude Code message loading complete"
    );

    Ok(messages)
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

fn timestamp_value_to_utc(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let value = value?;
    match value {
        Value::String(text) => {
            parse_utc(text).or_else(|| text.parse::<i64>().ok().and_then(millis_to_utc))
        }
        Value::Number(_) => value_i64(Some(value)).and_then(millis_to_utc),
        Value::Object(map) => ["timestamp", "createdAt", "value"]
            .iter()
            .find_map(|field| timestamp_value_to_utc(map.get(*field))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
