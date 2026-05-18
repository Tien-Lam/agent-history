use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{Provider, Session, SessionId};
use crate::provider::parse_common::parse_utc;
use crate::provider::project_name_from_path;

#[derive(Deserialize)]
struct WorkspaceYaml {
    id: Option<String>,
    cwd: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub(crate) fn build_session(session_dir: &Path, workspace_path: &Path) -> Option<Session> {
    let yaml_content = std::fs::read_to_string(workspace_path).ok()?;
    let workspace: WorkspaceYaml = serde_yaml_ng::from_str(&yaml_content).ok()?;

    let session_id = workspace.id.or_else(|| {
        session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    })?;

    let started_at = workspace.created_at.as_deref().and_then(parse_utc)?;

    let ended_at = workspace.updated_at.as_deref().and_then(parse_utc);

    let project_name = workspace.cwd.as_deref().and_then(project_name_from_path);
    let project_path = workspace.cwd.map(PathBuf::from);

    let events_path = session_dir.join("events.jsonl");
    let message_count = if events_path.exists() {
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

fn count_message_events(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let reader = BufReader::new(file);
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| {
            l.contains("\"user.message\"")
                || l.contains("\"assistant.message\"")
                || l.contains("\"tool.execution_start\"")
                || l.contains("\"tool.execution_complete\"")
                || l.contains("\"tool.invoke\"")
                || l.contains("\"tool.result\"")
        })
        .count()
}
