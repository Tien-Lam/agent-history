use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::fs_read;
use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{
    file_modified_utc, parse_utc_opt, unix_epoch_utc, MAX_PROVIDER_METADATA_FILE_BYTES,
};

use super::messages::parse_jsonl;

pub(crate) const INDEX_FILE: &str = "index.json";

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
    let bytes = fs_read::read_regular_file_limited(
        &sessions_dir.join(INDEX_FILE),
        MAX_PROVIDER_METADATA_FILE_BYTES,
    )
    .ok()?;
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
        .unwrap_or_else(unix_epoch_utc);

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
