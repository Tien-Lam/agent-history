use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::anthropic_content::{content_to_blocks, AnthropicContent};

pub(crate) const INDEX_FILE: &str = "index.json";

#[derive(Deserialize)]
struct SessionLine {
    role: String,
    #[serde(default)]
    content: AnthropicContent,
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

        let blocks = content_to_blocks(parsed.content);
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
