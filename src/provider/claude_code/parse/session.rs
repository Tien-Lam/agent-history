use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::{stringish, value_u64};
use crate::provider::parse_common::{
    nonzero_token_usage, parse_jsonl_records, visit_jsonl_records,
};

use super::{claude_timestamp, ProviderError, RawSessionEntry};

#[derive(Deserialize)]
pub(crate) struct HistoryEntry {
    display: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "sessionId")]
    session_id: Option<Value>,
}

pub(crate) fn parse_history_index(path: &Path) -> Result<Vec<HistoryEntry>, ProviderError> {
    Ok(parse_jsonl_records::<HistoryEntry>(path)?
        .records
        .into_iter()
        .map(|record| record.value)
        .collect())
}

/// Decode the project directory name back to a readable path.
/// Claude Code encodes `V:\Projects\agent-history` as `V--Projects-agent-history`.
/// The encoding is lossy (both `/` and literal `-` become `-`), so we only
/// decode `--` (drive separator) and leave single dashes as-is.
pub(crate) fn decode_project_name(encoded: &str) -> String {
    encoded.replace("--", ":/")
}

pub(crate) fn build_session_metadata(
    source_path: &Path,
    session_id: &str,
    project_name: &str,
    history_entries: &[HistoryEntry],
) -> Option<Session> {
    // Quick scan of the session file for timestamps and message count
    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    visit_jsonl_records::<RawSessionEntry, _, _>(
        source_path,
        |record| {
            let entry = record.value;
            if let Some(dt) = claude_timestamp(entry.timestamp.as_ref()) {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }

            let entry_role = stringish(entry.entry_type.as_ref(), &["type"]);
            if let Some("user" | "assistant") = entry_role.as_deref() {
                message_count += 1;

                if git_branch.is_none() {
                    if let Some(branch) =
                        stringish(entry.git_branch.as_ref(), &["gitBranch", "branch"])
                    {
                        git_branch = Some(branch);
                    }
                }
                if cwd.is_none() {
                    if let Some(c) = stringish(entry.cwd.as_ref(), &["cwd", "path"]) {
                        cwd = Some(c);
                    }
                }

                if entry_role.as_deref() == Some("assistant") {
                    if let Some(ref msg) = entry.message {
                        if model.is_none() {
                            if let Some(m) = stringish(msg.model.as_ref(), &["model", "id", "name"])
                            {
                                model = Some(m);
                            }
                        }
                        if let Some(ref usage) = msg.usage {
                            total_input_tokens +=
                                value_u64(usage.input_tokens.as_ref()).unwrap_or(0);
                            total_output_tokens +=
                                value_u64(usage.output_tokens.as_ref()).unwrap_or(0);
                        }
                    }
                }
            }
        },
        |_| {},
    )
    .ok()?;

    if message_count == 0 {
        return None;
    }

    // Get first user message as summary from history entries
    let summary = history_entries
        .iter()
        .find(|e| {
            stringish(e.session_id.as_ref(), &["sessionId", "id"]).as_deref() == Some(session_id)
        })
        .and_then(|e| stringish(e.display.as_ref(), &["display", "text", "content"]));

    // Use history entry timestamp if we didn't find one in the session file
    let started_at = first_timestamp.or_else(|| {
        history_entries
            .iter()
            .find(|e| {
                stringish(e.session_id.as_ref(), &["sessionId", "id"]).as_deref()
                    == Some(session_id)
            })
            .and_then(|e| claude_timestamp(e.timestamp.as_ref()))
    })?;

    let token_usage = nonzero_token_usage(total_input_tokens, total_output_tokens, None, None);

    Some(Session {
        id: SessionId(session_id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: cwd.map(PathBuf::from),
        project_name: Some(project_name.to_string()),
        git_branch,
        started_at,
        ended_at: last_timestamp,
        summary,
        model,
        token_usage,
        message_count,
        source_path: source_path.to_path_buf(),
    })
}
