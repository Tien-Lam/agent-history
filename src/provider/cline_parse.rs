use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, ToolCall, ToolResult,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) const API_HISTORY_FILE: &str = "api_conversation_history.json";
pub(crate) const UI_MESSAGES_FILE: &str = "ui_messages.json";
pub(crate) const METADATA_FILE: &str = "task_metadata.json";

#[derive(Deserialize)]
struct ApiMessage {
    role: String,
    #[serde(default)]
    content: ApiContent,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum ApiContent {
    Blocks(Vec<ApiBlock>),
    Text(String),
    #[default]
    Empty,
}

#[derive(Deserialize)]
struct ApiBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    // tool_use
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    // tool_result
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UiMessage {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct TaskMetadata {
    #[serde(rename = "createdAt", default)]
    created_at: Option<i64>,
}

pub(crate) fn parse_task_dir(path: &Path) -> Option<Session> {
    let task_id = path.file_name()?.to_str()?.to_string();

    let history_path = path.join(API_HISTORY_FILE);
    if !history_path.exists() {
        return None;
    }

    let started_at = started_at_for(path, &task_id);
    let summary = task_summary(path);

    Some(Session {
        id: SessionId(task_id),
        provider: Provider::Cline,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at,
        ended_at: None,
        summary,
        model: None,
        token_usage: None,
        message_count: 0,
        source_path: path.to_path_buf(),
    })
}

/// Parse the session start timestamp. Priority:
/// 1. `task_metadata.json` `createdAt` (ms since epoch)
/// 2. Task directory name if it looks like an ms-epoch integer
/// 3. Directory mtime
fn started_at_for(path: &Path, task_id: &str) -> DateTime<Utc> {
    let meta_path = path.join(METADATA_FILE);
    if let Ok(bytes) = std::fs::read(&meta_path) {
        if let Ok(meta) = serde_json::from_slice::<TaskMetadata>(&bytes) {
            if let Some(ms) = meta.created_at {
                let secs = ms / 1000;
                let nsecs = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
                if let Some(dt) = Utc.timestamp_opt(secs, nsecs).single() {
                    return dt;
                }
            }
        }
    }

    if let Ok(ms) = task_id.parse::<i64>() {
        let secs = ms / 1000;
        let nsecs = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
        if let Some(dt) = Utc.timestamp_opt(secs, nsecs).single() {
            return dt;
        }
    }

    path.metadata()
        .and_then(|m| m.modified())
        .map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
}

/// Extract a human-readable summary from `ui_messages.json` first entry's text.
fn task_summary(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path.join(UI_MESSAGES_FILE)).ok()?;
    let msgs: Vec<UiMessage> = serde_json::from_slice(&bytes).ok()?;
    let text = msgs.into_iter().find_map(|m| m.text)?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(if text.chars().count() > 120 {
        format!("{}…", text.chars().take(119).collect::<String>())
    } else {
        text
    })
}

pub(crate) fn parse_api_history(
    path: &Path,
    base_ts: &DateTime<Utc>,
) -> Result<Vec<Message>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let raw: Vec<ApiMessage> = serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;

    let mut messages = Vec::with_capacity(raw.len());
    for (idx, msg) in raw.into_iter().enumerate() {
        let role = match msg.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };

        let blocks = api_content_to_blocks(msg.content);
        if blocks.is_empty() {
            continue;
        }

        // Spread messages 1 ms apart so ordering is stable even without embedded timestamps
        let timestamp = *base_ts + chrono::Duration::milliseconds(i64::try_from(idx).unwrap_or(0));

        messages.push(Message {
            id: MessageId(format!("msg-{idx}")),
            role,
            timestamp,
            content: blocks,
            model: None,
            token_usage: None,
        });
    }

    Ok(messages)
}

fn api_content_to_blocks(content: ApiContent) -> Vec<ContentBlock> {
    match content {
        ApiContent::Text(t) if !t.trim().is_empty() => parse_text_with_code_blocks(&t),
        ApiContent::Blocks(blocks) => blocks.into_iter().flat_map(api_block_to_content).collect(),
        _ => vec![],
    }
}

fn api_block_to_content(block: ApiBlock) -> Vec<ContentBlock> {
    match block.kind.as_str() {
        "text" => {
            let t = block.text.unwrap_or_default();
            if t.trim().is_empty() {
                vec![]
            } else {
                parse_text_with_code_blocks(&t)
            }
        }
        "tool_use" => {
            let id = block.id.unwrap_or_default();
            let name = block.name.unwrap_or_default();
            let arguments = block
                .input
                .map(|v| {
                    if let serde_json::Value::String(s) = v {
                        s
                    } else {
                        serde_json::to_string_pretty(&v).unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            vec![ContentBlock::ToolUse(ToolCall {
                id,
                name,
                arguments,
            })]
        }
        "tool_result" => {
            let tool_call_id = block.tool_use_id.unwrap_or_default();
            let output = block
                .content
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Array(arr) => arr
                        .into_iter()
                        .filter_map(|b| {
                            if b.get("type")?.as_str()? == "text" {
                                b.get("text")?.as_str().map(str::to_string)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                })
                .unwrap_or_default();
            vec![ContentBlock::ToolResult(ToolResult {
                tool_call_id,
                success: true,
                output,
            })]
        }
        _ => vec![],
    }
}
