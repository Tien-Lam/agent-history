use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::parse_common::{
    parse_millis_or_utc, parse_millis_or_utc_or_now, pretty_json_opt, token_usage_from_options,
    tool_result_block, tool_use_block,
};
use crate::provider::project_name_from_path;
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_session_from_file(path: &Path, storage_base: &Path) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;

    // Try new format (time.created as millis) first, then legacy (createdAt as ISO string)
    let started_at = parse_millis_or_utc(
        raw.time.as_ref().and_then(|t| t.created),
        raw.created_at.as_deref(),
    )?;

    let ended_at = parse_millis_or_utc(
        raw.time.as_ref().and_then(|t| t.updated),
        raw.updated_at.as_deref(),
    );

    // New format uses "directory", legacy uses "cwd"
    let project_string = raw.directory.or(raw.cwd);
    let project_name = project_string.as_deref().and_then(project_name_from_path);
    let project_path = project_string.map(PathBuf::from);

    // Extract model from new format
    let model = raw.model.and_then(|m| m.model_id);

    // Count messages in the message directory
    let message_dir = storage_base.join("message").join(&raw.id);
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
        id: SessionId(raw.id),
        provider: Provider::OpenCode,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw.title,
        model,
        token_usage: None,
        message_count,
        source_path: storage_base.to_path_buf(),
    })
}

pub(crate) fn parse_message_file(path: &Path, part_dir: &Path) -> Option<Message> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawMessage = serde_json::from_str(&data).ok()?;

    let role = match raw.role.as_deref() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return None,
    };

    // Try new format (time.created as millis) first, then legacy (timestamp as ISO string)
    let timestamp = parse_millis_or_utc_or_now(
        raw.time.as_ref().and_then(|t| t.created),
        raw.timestamp.as_deref(),
    );

    let msg_id = raw.id.clone().unwrap_or_default();
    let mut content = Vec::new();

    // Try loading parts from part/{messageID}/ directory (new format)
    let msg_part_dir = part_dir.join(&msg_id);
    if msg_part_dir.exists() {
        load_parts_into_content(&msg_part_dir, &mut content);
    }

    // Fall back to legacy fields if no parts found
    if content.is_empty() {
        if let Some(text) = &raw.content {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(text));
            }
        }

        if let Some(changes) = &raw.code_changes {
            for change in changes {
                let label = change.path.as_deref().unwrap_or("diff");
                let diff = change.diff.as_deref().unwrap_or("");
                if !diff.is_empty() {
                    content.push(ContentBlock::CodeBlock {
                        language: Some(format!("diff ({label})")),
                        code: diff.to_string(),
                    });
                }
            }
        }
    }

    // If still no content, try summary.title (new format user messages)
    if content.is_empty() {
        if let Some(ref summary) = raw.summary {
            if let Some(ref title) = summary.title {
                if !title.is_empty() {
                    content.push(ContentBlock::Text(title.clone()));
                }
            }
        }
    }

    if content.is_empty() {
        return None;
    }

    let token_usage = raw.tokens.as_ref().map(|t| {
        token_usage_from_options(
            t.input,
            t.output,
            t.cache.as_ref().and_then(|c| c.read),
            t.cache.as_ref().and_then(|c| c.write),
        )
    });

    let model = raw.model.and_then(|m| m.model_id);

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
        match part.part_type.as_str() {
            "text" => {
                if let Some(ref text) = part.text {
                    if !text.is_empty() {
                        content.extend(parse_text_with_code_blocks(text));
                    }
                }
            }
            "tool" => {
                let tool_name = part.tool.clone().unwrap_or_else(|| "unknown".to_string());
                let call_id = part.call_id.clone().unwrap_or_default();
                let arguments = pretty_json_opt(part.state.as_ref().and_then(|s| s.input.as_ref()));
                content.push(tool_use_block(call_id, tool_name, arguments));

                // Include tool output as a result
                if let Some(ref state) = part.state {
                    if let Some(ref output) = state.output {
                        if !output.is_empty() {
                            let tool_call_id = part.call_id.clone().unwrap_or_default();
                            let success = state.status.as_deref() == Some("completed");
                            content.push(tool_result_block(tool_call_id, success, output.clone()));
                        }
                    }
                }
            }
            // Skip step-start, step-finish, and other structural types
            _ => {}
        }
    }
}

#[derive(Deserialize)]
struct RawSession {
    id: String,
    title: Option<String>,
    /// New format: "directory"
    directory: Option<String>,
    /// Legacy format: "cwd"
    cwd: Option<String>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: ISO timestamp strings
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    /// New format: model info
    model: Option<RawModel>,
}

#[derive(Deserialize)]
struct RawTime {
    created: Option<i64>,
    updated: Option<i64>,
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(rename = "modelID")]
    model_id: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    role: Option<String>,
    /// Legacy format: ISO timestamp
    timestamp: Option<String>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: text content
    content: Option<String>,
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
    title: Option<String>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<u64>,
    output: Option<u64>,
    cache: Option<RawCache>,
}

#[derive(Deserialize)]
struct RawCache {
    read: Option<u64>,
    write: Option<u64>,
}

#[derive(Deserialize)]
struct RawCodeChange {
    path: Option<String>,
    diff: Option<String>,
}

#[derive(Deserialize)]
struct RawPart {
    #[serde(rename = "type")]
    part_type: String,
    /// Text content (for type="text")
    text: Option<String>,
    /// Tool name (for type="tool")
    tool: Option<String>,
    /// Tool call ID (for type="tool")
    #[serde(rename = "callID")]
    call_id: Option<String>,
    /// Tool state with input/output (for type="tool")
    state: Option<RawToolState>,
}

#[derive(Deserialize)]
struct RawToolState {
    status: Option<String>,
    input: Option<serde_json::Value>,
    output: Option<String>,
}
