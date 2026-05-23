use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::stringish;
use crate::provider::project_name_from_path;

use super::{timestamp_from_values, RawModel, RawTime};

#[derive(Deserialize)]
struct RawSession {
    id: Option<Value>,
    title: Option<Value>,
    /// New format: "directory"
    directory: Option<Value>,
    /// Legacy format: "cwd"
    cwd: Option<Value>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: ISO timestamp strings
    #[serde(rename = "createdAt")]
    created_at: Option<Value>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<Value>,
    /// New format: model info
    model: Option<RawModel>,
}

pub(crate) fn build_session_from_file(path: &Path, storage_base: &Path) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;
    let id = stringish(raw.id.as_ref(), &["id"]).or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
    })?;

    // Try new format (time.created as millis) first, then legacy (createdAt as ISO string)
    let started_at = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.created.as_ref()),
        raw.created_at.as_ref(),
    )?;

    let ended_at = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.updated.as_ref()),
        raw.updated_at.as_ref(),
    );

    // New format uses "directory", legacy uses "cwd"
    let project_string = stringish(raw.directory.as_ref(), &["directory", "path"])
        .or_else(|| stringish(raw.cwd.as_ref(), &["cwd", "path"]));
    let project_name = project_string.as_deref().and_then(project_name_from_path);
    let project_path = project_string.map(PathBuf::from);

    // Extract model from new format
    let model = raw
        .model
        .and_then(|m| stringish(m.model_id.as_ref(), &["modelID", "model", "id"]));

    // Count messages in the message directory
    let message_dir = storage_base.join("message").join(&id);
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
        id: SessionId(id),
        provider: Provider::OpenCode,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: stringish(raw.title.as_ref(), &["title", "text", "content"]),
        model,
        token_usage: None,
        message_count,
        source_path: storage_base.to_path_buf(),
    })
}
