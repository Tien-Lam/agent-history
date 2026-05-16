use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::ProviderError;
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
    ToolResult,
};
use crate::provider::json_text::string_or_typed_text_array;
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Deserialize)]
pub(crate) struct HistoryEntry {
    display: Option<String>,
    timestamp: Option<u64>,
    #[allow(dead_code)]
    project: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

pub(crate) fn parse_history_index(path: &Path) -> Result<Vec<HistoryEntry>, ProviderError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
            entries.push(entry);
        }
    }

    Ok(entries)
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
    let file = std::fs::File::open(source_path).ok()?;
    let reader = BufReader::new(file);

    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }

        let entry: RawSessionEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if let Some(ts) = &entry.timestamp {
            if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }
        }

        if let Some("user" | "assistant") = entry.entry_type.as_deref() {
            message_count += 1;

            if git_branch.is_none() {
                if let Some(ref branch) = entry.git_branch {
                    git_branch = Some(branch.clone());
                }
            }
            if cwd.is_none() {
                if let Some(ref c) = entry.cwd {
                    cwd = Some(c.clone());
                }
            }

            if entry.entry_type.as_deref() == Some("assistant") {
                if let Some(ref msg) = entry.message {
                    if model.is_none() {
                        if let Some(ref m) = msg.model {
                            model = Some(m.clone());
                        }
                    }
                    if let Some(ref usage) = msg.usage {
                        total_input_tokens += usage.input_tokens.unwrap_or(0);
                        total_output_tokens += usage.output_tokens.unwrap_or(0);
                    }
                }
            }
        }
    }

    if message_count == 0 {
        return None;
    }

    // Get first user message as summary from history entries
    let summary = history_entries
        .iter()
        .find(|e| e.session_id.as_deref() == Some(session_id))
        .and_then(|e| e.display.clone());

    // Use history entry timestamp if we didn't find one in the session file
    let started_at = first_timestamp.or_else(|| {
        history_entries
            .iter()
            .find(|e| e.session_id.as_deref() == Some(session_id))
            .and_then(|e| e.timestamp)
            .and_then(|ts| Utc.timestamp_millis_opt(ts.cast_signed()).single())
    })?;

    let token_usage = if total_input_tokens > 0 || total_output_tokens > 0 {
        Some(TokenUsage {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
        })
    } else {
        None
    };

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
    entry_type: Option<String>,
    uuid: Option<String>,
    timestamp: Option<String>,
    message: Option<RawMessage>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<serde_json::Value>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[allow(clippy::struct_field_names)]
#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

pub(crate) fn parse_session_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Claude Code messages");
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut line_count: usize = 0;
    let mut parse_errors: usize = 0;
    let mut skipped_types: usize = 0;
    let mut empty_content: usize = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        line_count += 1;

        let entry: RawSessionEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                parse_errors += 1;
                tracing::warn!(line_num = line_count, error = %e, "failed to parse JSONL line");
                continue;
            }
        };

        let role = match entry.entry_type.as_deref() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            Some(other) => {
                skipped_types += 1;
                tracing::trace!(entry_type = other, "skipping non-message entry");
                continue;
            }
            None => {
                skipped_types += 1;
                continue;
            }
        };

        let Some(ref msg) = entry.message else {
            tracing::warn!(role = ?role, uuid = ?entry.uuid, "entry has no message field");
            continue;
        };

        let timestamp = entry
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let id = entry.uuid.unwrap_or_default();

        let content = parse_message_content(msg, role);
        if content.is_empty() {
            empty_content += 1;
            tracing::trace!(msg_id = %id, role = ?role, "skipping message with empty content");
            continue;
        }

        let token_usage = msg.usage.as_ref().map(|u| TokenUsage {
            input_tokens: u.input_tokens.unwrap_or(0),
            output_tokens: u.output_tokens.unwrap_or(0),
            cache_read_tokens: u.cache_read_input_tokens,
            cache_write_tokens: u.cache_creation_input_tokens,
        });

        messages.push(Message {
            id: MessageId(id),
            role,
            timestamp,
            content,
            model: msg.model.clone(),
            token_usage,
        });
    }

    tracing::info!(
        path = %path.display(),
        lines = line_count,
        parse_errors,
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
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "text" => {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            blocks.extend(parse_text_with_code_blocks(text));
                        }
                    }
                    "thinking" => {
                        if let Some(text) = item.get("thinking").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Thinking(text.to_string()));
                            }
                        }
                    }
                    "tool_use" => {
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let arguments = item
                            .get("input")
                            .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                            .unwrap_or_default();
                        blocks.push(ContentBlock::ToolUse(ToolCall {
                            id,
                            name,
                            arguments,
                        }));
                    }
                    "tool_result" if role == Role::User => {
                        let tool_call_id = item
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let is_error = item
                            .get("is_error")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let output = extract_tool_result_text(item);
                        blocks.push(ContentBlock::ToolResult(ToolResult {
                            tool_call_id,
                            success: !is_error,
                            output,
                        }));
                    }
                    _ => {}
                }
            }
            blocks
        }
        _ => Vec::new(),
    }
}

fn extract_tool_result_text(item: &serde_json::Value) -> String {
    item.get("content")
        .map(|content| string_or_typed_text_array(content, "text", "text"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_project_name_basic() {
        assert_eq!(
            decode_project_name("V--Projects-agent-history"),
            "V:/Projects-agent-history"
        );
    }
}
