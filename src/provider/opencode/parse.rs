use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_u64};
use crate::provider::parse_common::{
    millis_to_utc, parse_utc_opt, pretty_json_opt, token_usage_from_options, tool_result_block,
    tool_use_block,
};
use crate::provider::project_name_from_path;
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_session_from_file(path: &Path, storage_base: &Path) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;
    let id = stringish(raw.id.as_ref(), &["id"]).or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
    })?;

    // Try new format (time.created as millis) first, then legacy (createdAt as ISO string)
    let started_at = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.created.as_ref()),
        raw.created_at.as_ref(),
    )?;

    let ended_at = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.updated.as_ref()),
        raw.updated_at.as_ref(),
    );

    // New format uses "directory", legacy uses "cwd"
    let project_string = stringish(raw.directory.as_ref(), &["directory", "path"])
        .or_else(|| stringish(raw.cwd.as_ref(), &["cwd", "path"]));
    let project_name = project_string.as_deref().and_then(project_name_from_path);
    let project_path = project_string.map(PathBuf::from);

    // Extract model from new format
    let model = raw
        .model
        .and_then(|m| stringish(m.model_id.as_ref(), &["modelID", "model", "id"]));

    // Count messages in the message directory
    let message_dir = storage_base.join("message").join(&id);
    let message_count = if message_dir.exists() {
        std::fs::read_dir(&message_dir).map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
                .count()
        })
    } else {
        0
    };

    Some(Session {
        id: SessionId(id),
        provider: Provider::OpenCode,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: stringish(raw.title.as_ref(), &["title", "text", "content"]),
        model,
        token_usage: None,
        message_count,
        source_path: storage_base.to_path_buf(),
    })
}

pub(crate) fn parse_message_file(path: &Path, part_dir: &Path) -> Option<Message> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawMessage = serde_json::from_str(&data).ok()?;

    let role = match stringish(raw.role.as_ref(), &["role", "type"]).as_deref() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return None,
    };

    // Try new format (time.created as millis) first, then legacy (timestamp as ISO string)
    let timestamp = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.created.as_ref()),
        raw.timestamp.as_ref(),
    )
    .unwrap_or_else(chrono::Utc::now);

    let msg_id = stringish(raw.id.as_ref(), &["id"]).unwrap_or_default();
    let mut content = Vec::new();

    // Try loading parts from part/{messageID}/ directory (new format)
    let msg_part_dir = part_dir.join(&msg_id);
    if msg_part_dir.exists() {
        load_parts_into_content(&msg_part_dir, &mut content);
    }

    // Fall back to legacy fields if no parts found
    if content.is_empty() {
        if let Some(text) = raw.content.as_ref().map(message_text) {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(&text));
            }
        }

        if let Some(changes) = &raw.code_changes {
            for change in changes {
                let label = change
                    .path
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["path", "file"]))
                    .unwrap_or_else(|| "diff".to_string());
                let diff = change.diff.as_ref().map(message_text).unwrap_or_default();
                if !diff.is_empty() {
                    content.push(ContentBlock::CodeBlock {
                        language: Some(format!("diff ({label})")),
                        code: diff,
                    });
                }
            }
        }
    }

    // If still no content, try summary.title (new format user messages)
    if content.is_empty() {
        if let Some(ref summary) = raw.summary {
            if let Some(title) = summary.title.as_ref().map(message_text) {
                if !title.is_empty() {
                    content.push(ContentBlock::Text(title));
                }
            }
        }
    }

    if content.is_empty() {
        return None;
    }

    let token_usage = raw.tokens.as_ref().map(|t| {
        token_usage_from_options(
            value_u64(t.input.as_ref()),
            value_u64(t.output.as_ref()),
            t.cache.as_ref().and_then(|c| value_u64(c.read.as_ref())),
            t.cache.as_ref().and_then(|c| value_u64(c.write.as_ref())),
        )
    });

    let model = raw
        .model
        .and_then(|m| stringish(m.model_id.as_ref(), &["modelID", "model", "id"]));

    Some(Message {
        id: MessageId(msg_id),
        role,
        timestamp,
        content,
        model,
        token_usage,
    })
}

/// Load content blocks from part files in a message's part directory.
fn load_parts_into_content(part_dir: &Path, content: &mut Vec<ContentBlock>) {
    let Ok(entries) = std::fs::read_dir(part_dir) else {
        return;
    };

    let mut parts: Vec<(String, RawPart)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let Ok(data) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(part) = serde_json::from_str::<RawPart>(&data) else {
            continue;
        };

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        parts.push((filename, part));
    }

    // Sort by filename (part IDs are roughly chronological)
    parts.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, part) in &parts {
        match stringish(part.part_type.as_ref(), &["type"])
            .as_deref()
            .unwrap_or("")
        {
            "text" => {
                if let Some(text) = part.text.as_ref().map(message_text) {
                    if !text.is_empty() {
                        content.extend(parse_text_with_code_blocks(&text));
                    }
                }
            }
            "tool" => {
                let tool_name = part
                    .tool
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["name", "tool"]))
                    .unwrap_or_else(|| "unknown".to_string());
                let call_id =
                    stringish(part.call_id.as_ref(), &["callID", "id"]).unwrap_or_default();
                let arguments = pretty_json_opt(part.state.as_ref().and_then(|s| s.input.as_ref()));
                content.push(tool_use_block(call_id, tool_name, arguments));

                // Include tool output as a result
                if let Some(ref state) = part.state {
                    if let Some(output) = state.output.as_ref().map(tool_output_text) {
                        if !output.is_empty() {
                            let tool_call_id = stringish(part.call_id.as_ref(), &["callID", "id"])
                                .unwrap_or_default();
                            let success = stringish(state.status.as_ref(), &["status", "state"])
                                .as_deref()
                                == Some("completed");
                            content.push(tool_result_block(tool_call_id, success, output));
                        }
                    }
                }
            }
            // Skip step-start, step-finish, and other structural types
            _ => {}
        }
    }
}

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
    timestamp_value_to_utc(millis).or_else(|| timestamp_value_to_utc(raw))
}

fn timestamp_value_to_utc(value: Option<&Value>) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = value?;
    match value {
        Value::String(text) => {
            parse_utc_opt(Some(text)).or_else(|| text.parse::<i64>().ok().and_then(millis_to_utc))
        }
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|n| i64::try_from(n).ok()))
            .and_then(millis_to_utc),
        Value::Object(map) => ["created", "updated", "timestamp", "value"]
            .iter()
            .find_map(|field| timestamp_value_to_utc(map.get(*field))),
        _ => None,
    }
}

#[derive(Deserialize)]
struct RawSession {
    id: Option<Value>,
    title: Option<Value>,
    /// New format: "directory"
    directory: Option<Value>,
    /// Legacy format: "cwd"
    cwd: Option<Value>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: ISO timestamp strings
    #[serde(rename = "createdAt")]
    created_at: Option<Value>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<Value>,
    /// New format: model info
    model: Option<RawModel>,
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
