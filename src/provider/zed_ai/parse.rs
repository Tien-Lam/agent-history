use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::ProviderError;
use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish};
use crate::provider::parse_common::{
    epoch_timestamp_for_index, file_modified_utc, timestamp_value_to_utc,
};
use crate::provider::project_name_from_path;
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

#[derive(Debug, Deserialize)]
struct ZedConversation {
    id: Option<Value>,
    summary: Option<Value>,
    model: Option<Value>,
    workspace: Option<Value>,
    #[serde(default, alias = "createdAt")]
    created_at: Option<Value>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Value>,
    #[serde(default)]
    messages: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct ZedMessage {
    id: Option<Value>,
    role: Option<Value>,
    #[serde(default, alias = "content")]
    text: Option<Value>,
    #[serde(default, alias = "createdAt")]
    timestamp: Option<Value>,
    model: Option<Value>,
}

pub(crate) fn read_session(path: &Path) -> Result<Option<Session>, ProviderError> {
    let bytes = std::fs::read(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let raw: ZedConversation = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to parse Zed conversation JSON");
            return Ok(None);
        }
    };

    // Recover an ID: explicit field → file stem → skip.
    let id = raw
        .id
        .as_ref()
        .and_then(|value| stringish(Some(value), &["id"]))
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        });
    let Some(id) = id else { return Ok(None) };

    // Started_at: created_at → earliest message timestamp → file mtime → skip.
    let started_at = raw
        .created_at
        .as_ref()
        .and_then(|value| zed_timestamp(Some(value)))
        .or_else(|| earliest_message_ts(&raw))
        .or_else(|| file_modified_utc(path));
    let Some(started_at) = started_at else {
        return Ok(None);
    };

    let ended_at = raw
        .updated_at
        .as_ref()
        .and_then(|value| zed_timestamp(Some(value)))
        .or_else(|| latest_message_ts(&raw));

    let workspace = raw
        .workspace
        .as_ref()
        .and_then(|value| stringish(Some(value), &["path", "workspace"]));
    let project_path = workspace.clone().map(PathBuf::from);
    let project_name = workspace.as_deref().and_then(project_name_from_path);

    let message_count = raw.messages.len();

    Ok(Some(Session {
        id: SessionId(id),
        provider: Provider::ZedAi,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw
            .summary
            .as_ref()
            .and_then(|value| stringish(Some(value), &["summary", "title", "text"])),
        model: raw
            .model
            .as_ref()
            .and_then(|value| stringish(Some(value), &["model", "id", "name"])),
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    }))
}

pub(crate) fn load_messages_from_path_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    let bytes = std::fs::read(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let raw: ZedConversation =
        serde_json::from_slice(&bytes).map_err(|e| ProviderError::Parse {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let mut parse_stats = ProviderParseStats::default();
    let messages = raw
        .messages
        .iter()
        .enumerate()
        .filter_map(|(idx, raw_message)| {
            parse_stats.record_seen();
            let Ok(m) = serde_json::from_value::<ZedMessage>(raw_message.clone()) else {
                parse_stats.record_parse_error();
                return None;
            };
            let role_text = m
                .role
                .as_ref()
                .and_then(|value| stringish(Some(value), &["role"]));
            if parse_role(role_text.as_deref()).is_none() {
                parse_stats.record_skipped_record();
                return None;
            }

            let message = build_message(&m, idx)?;
            if message.content.is_empty() {
                parse_stats.record_empty_content();
            }
            Some(message)
        })
        .collect();
    Ok(ProviderMessageLoad {
        messages,
        parse_stats,
    })
}

fn earliest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| serde_json::from_value::<ZedMessage>(m.clone()).ok())
        .filter_map(|m| zed_timestamp(m.timestamp.as_ref()))
        .min()
}

fn latest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| serde_json::from_value::<ZedMessage>(m.clone()).ok())
        .filter_map(|m| zed_timestamp(m.timestamp.as_ref()))
        .max()
}

fn build_message(raw: &ZedMessage, idx: usize) -> Option<Message> {
    let role_text = raw
        .role
        .as_ref()
        .and_then(|value| stringish(Some(value), &["role"]));
    let role = parse_role(role_text.as_deref())?;
    let body = raw.text.as_ref().map(message_text).unwrap_or_default();

    let timestamp = raw
        .timestamp
        .as_ref()
        .and_then(|value| zed_timestamp(Some(value)))
        .unwrap_or_else(|| epoch_timestamp_for_index(idx));

    let id = raw
        .id
        .as_ref()
        .and_then(|value| stringish(Some(value), &["id"]))
        .unwrap_or_else(|| format!("zed-msg-{idx}"));
    let content = if body.is_empty() {
        Vec::new()
    } else {
        parse_text_with_code_blocks(&body)
    };

    Some(Message {
        id: MessageId(id),
        role,
        timestamp,
        content,
        model: raw
            .model
            .as_ref()
            .and_then(|value| stringish(Some(value), &["model", "id", "name"])),
        token_usage: None,
    })
}

fn message_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["text", "content", "message"])
}

fn zed_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(
        value,
        &[
            "timestamp",
            "time",
            "createdAt",
            "created_at",
            "updatedAt",
            "updated_at",
            "value",
        ],
    )
}

fn parse_role(role: Option<&str>) -> Option<Role> {
    match role?.to_ascii_lowercase().as_str() {
        "user" | "human" => Some(Role::User),
        "assistant" | "model" => Some(Role::Assistant),
        "system" => Some(Role::System),
        "tool" => Some(Role::Tool),
        _ => None,
    }
}
