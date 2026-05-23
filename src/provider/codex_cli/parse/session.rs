use std::path::Path;

use chrono::{DateTime, Utc};

use crate::model::{Provider, Session, SessionId};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{timestamp_value_to_utc, visit_jsonl_records};

use super::{entry_text, RawEntry};

pub(crate) fn build_session_from_rollout(path: &Path) -> Option<Session> {
    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut first_user_message: Option<String> = None;

    visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let entry = record.value;
            if let Some(dt) =
                timestamp_value_to_utc(entry.timestamp.as_ref(), &["timestamp", "time", "value"])
            {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }

            let entry_type = stringish(entry.entry_type.as_ref(), &["type"]);
            match entry_type.as_deref() {
                Some("user" | "assistant") => {
                    message_count += 1;
                    if entry_type.as_deref() == Some("user") && first_user_message.is_none() {
                        first_user_message = entry
                            .content
                            .as_ref()
                            .map(entry_text)
                            .map(|c| c.chars().take(80).collect());
                    }
                }
                Some("event_msg") => {
                    // Newer Codex format
                    if let Some(ref payload) = entry.payload {
                        let payload_type = stringish(payload.entry_type.as_ref(), &["type"]);
                        if let Some("user_message" | "agent_message") = payload_type.as_deref() {
                            message_count += 1;
                            if payload_type.as_deref() == Some("user_message")
                                && first_user_message.is_none()
                            {
                                first_user_message = payload
                                    .message
                                    .as_ref()
                                    .map(entry_text)
                                    .map(|m| m.chars().take(80).collect());
                            }
                        }
                    }
                }
                _ => {}
            }
        },
        |_| {},
    )
    .ok()?;

    if message_count == 0 {
        return None;
    }

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::CodexCli,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at: first_timestamp?,
        ended_at: last_timestamp,
        summary: first_user_message,
        model: None,
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    })
}
