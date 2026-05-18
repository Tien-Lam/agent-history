use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::ProviderError;
use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::parse_common::{epoch_timestamp_for_index, file_modified_utc, millis_to_utc};
use crate::provider::project_name_from_path;
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Debug, Deserialize)]
struct ZedConversation {
    id: Option<String>,
    summary: Option<String>,
    model: Option<String>,
    workspace: Option<String>,
    #[serde(default, alias = "createdAt")]
    created_at: Option<Timestamp>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Timestamp>,
    #[serde(default)]
    messages: Vec<ZedMessage>,
}

#[derive(Debug, Deserialize)]
struct ZedMessage {
    id: Option<String>,
    role: Option<String>,
    #[serde(default, alias = "content")]
    text: Option<String>,
    #[serde(default, alias = "createdAt")]
    timestamp: Option<Timestamp>,
    model: Option<String>,
}

/// Accepts either an RFC3339 string or epoch milliseconds. Older Zed builds
/// recorded millis; newer ones emit ISO strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Timestamp {
    Iso(String),
    Millis(i64),
}

impl Timestamp {
    fn to_utc(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Iso(s) => DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc)),
            Self::Millis(m) => millis_to_utc(*m),
        }
    }
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
    let id = raw.id.clone().or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    });
    let Some(id) = id else { return Ok(None) };

    // Started_at: created_at → earliest message timestamp → file mtime → skip.
    let started_at = raw
        .created_at
        .as_ref()
        .and_then(Timestamp::to_utc)
        .or_else(|| earliest_message_ts(&raw))
        .or_else(|| file_modified_utc(path));
    let Some(started_at) = started_at else {
        return Ok(None);
    };

    let ended_at = raw
        .updated_at
        .as_ref()
        .and_then(Timestamp::to_utc)
        .or_else(|| latest_message_ts(&raw));

    let project_path = raw.workspace.clone().map(PathBuf::from);
    let project_name = raw.workspace.as_deref().and_then(project_name_from_path);

    let message_count = raw.messages.len();

    Ok(Some(Session {
        id: SessionId(id),
        provider: Provider::ZedAi,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw.summary,
        model: raw.model,
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    }))
}

pub(crate) fn load_messages_from_path(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let bytes = std::fs::read(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let raw: ZedConversation =
        serde_json::from_slice(&bytes).map_err(|e| ProviderError::Parse {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let messages = raw
        .messages
        .into_iter()
        .enumerate()
        .filter_map(|(idx, m)| build_message(m, idx))
        .collect();
    Ok(messages)
}

fn earliest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| m.timestamp.as_ref().and_then(Timestamp::to_utc))
        .min()
}

fn latest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| m.timestamp.as_ref().and_then(Timestamp::to_utc))
        .max()
}

fn build_message(raw: ZedMessage, idx: usize) -> Option<Message> {
    let role = parse_role(raw.role.as_deref())?;
    let body = raw.text.unwrap_or_default();

    let timestamp = raw
        .timestamp
        .as_ref()
        .and_then(Timestamp::to_utc)
        .unwrap_or_else(|| epoch_timestamp_for_index(idx));

    let id = raw.id.unwrap_or_else(|| format!("zed-msg-{idx}"));
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
        model: raw.model,
        token_usage: None,
    })
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
