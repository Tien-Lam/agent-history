use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::fs_read;
use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{
    timestamp_value_to_utc, visit_jsonl_records, MAX_PROVIDER_METADATA_FILE_BYTES,
};
use crate::provider::{path_is_regular_file, project_name_from_path};

#[derive(Deserialize)]
struct WorkspaceYaml {
    id: Option<Value>,
    cwd: Option<Value>,
    created_at: Option<Value>,
    updated_at: Option<Value>,
}

pub(crate) fn build_session(session_dir: &Path, workspace_path: &Path) -> Option<Session> {
    let yaml_content = fs_read::read_regular_file_to_string_limited(
        workspace_path,
        MAX_PROVIDER_METADATA_FILE_BYTES,
    )
    .ok()?;
    let workspace: WorkspaceYaml = serde_yaml_ng::from_str(&yaml_content).ok()?;

    let session_id = stringish(workspace.id.as_ref(), &["id"]).or_else(|| {
        session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    })?;

    let started_at = copilot_workspace_timestamp(workspace.created_at.as_ref())?;

    let ended_at = copilot_workspace_timestamp(workspace.updated_at.as_ref());

    let cwd = stringish(workspace.cwd.as_ref(), &["cwd", "path", "workspace"]);
    let project_name = cwd.as_deref().and_then(project_name_from_path);
    let project_path = cwd.map(PathBuf::from);

    let events_path = session_dir.join("events.jsonl");
    let message_count = if path_is_regular_file(&events_path) {
        count_message_events(&events_path)
    } else {
        0
    };

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::CopilotCli,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: None,
        model: None,
        token_usage: None,
        message_count,
        source_path: session_dir.to_path_buf(),
    })
}

fn copilot_workspace_timestamp(value: Option<&Value>) -> Option<chrono::DateTime<chrono::Utc>> {
    timestamp_value_to_utc(
        value,
        &[
            "created_at",
            "createdAt",
            "updated_at",
            "updatedAt",
            "timestamp",
            "time",
            "value",
        ],
    )
}

fn count_message_events(path: &Path) -> usize {
    let mut count = 0;
    let _ = visit_jsonl_records::<Value, _, _>(
        path,
        |record| {
            let Some(event_type) = record
                .value
                .get("type")
                .and_then(|value| stringish(Some(value), &["type", "event"]))
            else {
                return;
            };
            if event_type.contains("user.message")
                || event_type.contains("assistant.message")
                || event_type == "tool.execution_start"
                || event_type == "tool.execution_complete"
                || event_type == "tool.invoke"
                || event_type == "tool.result"
            {
                count += 1;
            }
        },
        |_| {},
    );
    count
}
