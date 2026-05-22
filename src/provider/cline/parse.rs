use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::anthropic_content::{content_to_blocks, AnthropicContent};
use crate::provider::json_text::{stringish, value_i64};
use crate::provider::parse_common::{
    file_modified_utc, millis_to_utc, timestamp_with_index_millis,
};

pub(crate) const API_HISTORY_FILE: &str = "api_conversation_history.json";
pub(crate) const UI_MESSAGES_FILE: &str = "ui_messages.json";
pub(crate) const METADATA_FILE: &str = "task_metadata.json";

#[derive(Deserialize)]
struct ApiMessage {
    role: Option<Value>,
    #[serde(default)]
    content: AnthropicContent,
}

#[derive(Deserialize)]
struct UiMessage {
    #[serde(default)]
    text: Option<Value>,
}

#[derive(Deserialize)]
struct TaskMetadata {
    #[serde(rename = "createdAt", default)]
    created_at: Option<Value>,
}

pub(crate) fn parse_task_dir(path: &Path) -> Option<Session> {
    let task_id = path.file_name()?.to_str()?.to_string();

    let history_path = path.join(API_HISTORY_FILE);
    if !history_path.exists() {
        return None;
    }

    let started_at = started_at_for(path, &task_id);
    let summary = task_summary(path);
    let message_count = parse_api_history(&history_path, &started_at).map_or(0, |m| m.len());

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
        message_count,
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
            if let Some(dt) = meta.created_at.as_ref().and_then(millis_value_to_utc) {
                return dt;
            }
        }
    }

    if let Ok(ms) = task_id.parse::<i64>() {
        if let Some(dt) = millis_to_utc(ms) {
            return dt;
        }
    }

    file_modified_utc(path).unwrap_or_else(Utc::now)
}

/// Extract a human-readable summary from `ui_messages.json` first entry's text.
fn task_summary(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path.join(UI_MESSAGES_FILE)).ok()?;
    let entries: Vec<Value> = serde_json::from_slice(&bytes).ok()?;
    let text = entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<UiMessage>(entry).ok())
        .find_map(|m| stringish(m.text.as_ref(), &["text", "content", "message"]))?
        .trim()
        .to_string();
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
    let raw: Vec<Value> = serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;

    let mut messages = Vec::with_capacity(raw.len());
    for (idx, entry) in raw.into_iter().enumerate() {
        let Ok(msg) = serde_json::from_value::<ApiMessage>(entry) else {
            continue;
        };

        let role = match stringish(msg.role.as_ref(), &["role", "type"]).as_deref() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => continue,
        };

        let blocks = content_to_blocks(msg.content);
        if blocks.is_empty() {
            continue;
        }

        // Spread messages 1 ms apart so ordering is stable even without embedded timestamps
        let timestamp = timestamp_with_index_millis(*base_ts, idx);

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

fn millis_value_to_utc(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Number(_) | Value::String(_) => value_i64(Some(value)).and_then(millis_to_utc),
        Value::Object(map) => ["createdAt", "timestamp", "value"]
            .iter()
            .find_map(|field| map.get(*field).and_then(millis_value_to_utc)),
        _ => None,
    }
}
