use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::file_modified_utc;
use crate::provider::project_name_from_path;

use super::{zed_timestamp, ProviderError, ZedConversation, ZedMessage};

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

    // Recover an ID: explicit field -> file stem -> skip.
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

    // Started_at: created_at -> earliest message timestamp -> file mtime -> skip.
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
