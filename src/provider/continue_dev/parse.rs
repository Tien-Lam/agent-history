use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::anthropic_content::{content_to_blocks, AnthropicContent};
use crate::provider::json_text::string_or_object_field;
use crate::provider::parse_common::{
    file_modified_utc, parse_utc_opt, timestamp_with_index_millis, visit_jsonl_records,
};

pub(crate) const INDEX_FILE: &str = "index.json";

#[derive(Deserialize)]
struct SessionLine {
    role: Option<Value>,
    #[serde(default)]
    content: AnthropicContent,
}

/// One entry in `~/.continue/sessions/index.json`.
#[derive(Deserialize)]
pub(crate) struct IndexEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<Value>,
    #[serde(default)]
    title: Option<Value>,
    #[serde(rename = "dateCreated", default)]
    date_created: Option<Value>,
}

pub(crate) fn load_index(sessions_dir: &Path) -> Option<Vec<IndexEntry>> {
    let bytes = std::fs::read(sessions_dir.join(INDEX_FILE)).ok()?;
    let entries: Vec<Value> = serde_json::from_slice(&bytes).ok()?;
    Some(
        entries
            .into_iter()
            .filter_map(|entry| serde_json::from_value(entry).ok())
            .collect(),
    )
}

pub(crate) fn build_session_from_file(
    path: PathBuf,
    session_id: String,
    index: Option<&[IndexEntry]>,
) -> Session {
    let meta = index.and_then(|idx| {
        idx.iter().find(|e| {
            stringish(e.session_id.as_ref(), &["sessionId", "id"]).as_deref()
                == Some(session_id.as_str())
        })
    });

    let started_at = meta
        .and_then(|m| {
            stringish(
                m.date_created.as_ref(),
                &["dateCreated", "timestamp", "value"],
            )
            .and_then(|raw| parse_utc_opt(Some(raw.as_str())))
        })
        .or_else(|| file_modified_utc(&path))
        .unwrap_or_else(Utc::now);

    let summary = meta.and_then(|m| stringish(m.title.as_ref(), &["title", "text", "content"]));
    let message_count = parse_jsonl(&path, &started_at).map_or(0, |m| m.len());

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
        message_count,
        source_path: path,
    }
}

pub(crate) fn parse_jsonl(path: &Path, base_ts: &DateTime<Utc>) -> Result<Vec<Message>, String> {
    let mut messages = Vec::new();
    let mut skipped_roles: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<SessionLine, _, _>(
        path,
        |record| {
            let idx = record.line_number.saturating_sub(1);
            let parsed = record.value;
            let role = match stringish(parsed.role.as_ref(), &["role", "type"]).as_deref() {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                Some("system") => Role::System,
                _ => {
                    skipped_roles += 1;
                    return;
                }
            };

            let blocks = content_to_blocks(parsed.content);
            if blocks.is_empty() {
                empty_content += 1;
                return;
            }

            let timestamp = timestamp_with_index_millis(*base_ts, idx);

            messages.push(Message {
                id: MessageId(format!("msg-{idx}")),
                role,
                timestamp,
                content: blocks,
                model: None,
                token_usage: None,
            });
        },
        |error| {
            tracing::warn!(line_num = error.line_number, error = %error.error, "failed to parse Continue JSONL line");
        },
    )
    .map_err(|e| e.to_string())?;

    tracing::info!(
        path = %path.display(),
        lines = stats.line_count,
        parse_errors = stats.parse_errors,
        skipped_roles,
        empty_content,
        messages = messages.len(),
        "Continue.dev message loading complete"
    );

    Ok(messages)
}

fn stringish(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(_) => {
            let text = string_or_object_field(value, object_fields);
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}
