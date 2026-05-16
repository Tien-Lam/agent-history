use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, ToolCall, ToolResult,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) const INDEX_FILE: &str = "index.json";

#[derive(Deserialize)]
struct SessionLine {
    role: String,
    #[serde(default)]
    content: LineContent,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum LineContent {
    Text(String),
    Blocks(Vec<ContentBlock2>),
    #[default]
    Empty,
}

#[derive(Deserialize)]
struct ContentBlock2 {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

/// One entry in `~/.continue/sessions/index.json`.
#[derive(Deserialize)]
pub(crate) struct IndexEntry {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(rename = "dateCreated", default)]
    date_created: Option<String>,
}

pub(crate) fn load_index(sessions_dir: &Path) -> Option<Vec<IndexEntry>> {
    let bytes = std::fs::read(sessions_dir.join(INDEX_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn build_session_from_file(
    path: PathBuf,
    session_id: String,
    index: Option<&[IndexEntry]>,
) -> Session {
    let meta = index.and_then(|idx| idx.iter().find(|e| e.session_id == session_id));

    let started_at = meta
        .and_then(|m| m.date_created.as_deref())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .or_else(|| {
            path.metadata()
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .ok()
        })
        .unwrap_or_else(Utc::now);

    let summary = meta.and_then(|m| m.title.clone());

    Session {
        id: SessionId(session_id),
        provider: Provider::ContinueDev,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at,
        ended_at: None,
        summary,
        model: None,
        token_usage: None,
        message_count: 0,
        source_path: path,
    }
}

pub(crate) fn parse_jsonl(path: &Path, base_ts: &DateTime<Utc>) -> Result<Vec<Message>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let mut messages = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: SessionLine =
            serde_json::from_str(line).map_err(|e| format!("line {idx}: {e}"))?;

        let role = match parsed.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => continue,
        };

        let blocks = line_content_to_blocks(parsed.content);
        if blocks.is_empty() {
            continue;
        }

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

fn line_content_to_blocks(content: LineContent) -> Vec<ContentBlock> {
    match content {
        LineContent::Text(t) if !t.trim().is_empty() => parse_text_with_code_blocks(&t),
        LineContent::Blocks(blocks) => blocks.into_iter().flat_map(block2_to_content).collect(),
        _ => vec![],
    }
}

fn block2_to_content(block: ContentBlock2) -> Vec<ContentBlock> {
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
