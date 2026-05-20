use std::path::PathBuf;

use chrono::{TimeZone, Utc};

use super::*;
use crate::model::SessionId;

mod aggregate;
mod pricing;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn mk_session(
    id: &str,
    provider: Provider,
    model: Option<&str>,
    project: Option<&str>,
    usage: Option<TokenUsage>,
) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider,
        project_path: project.map(PathBuf::from),
        project_name: project.map(str::to_string),
        git_branch: None,
        started_at: ts(0),
        ended_at: None,
        summary: None,
        model: model.map(str::to_string),
        token_usage: usage,
        message_count: 1,
        source_path: PathBuf::from(format!("/tmp/{id}")),
    }
}
