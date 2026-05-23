use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{Provider, Role, Session, SessionId};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_u64};
use crate::provider::parse_common::nonzero_token_usage;

use super::{gemini_timestamp, raw_role, text_parts, RawContent, RawMessage, RawSession};

#[derive(Deserialize)]
struct ProjectsFile {
    projects: HashMap<String, String>,
}

pub(crate) fn load_project_map(base: &Path) -> HashMap<String, String> {
    let path = base.join("projects.json");
    if !path.exists() {
        return HashMap::new();
    }

    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<ProjectsFile>(&s).ok())
        .map(|pf| {
            // Reverse the map: slug -> path
            pf.projects
                .into_iter()
                .map(|(path, slug)| (slug, path))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn build_session_from_file(
    path: &Path,
    project_slug: &str,
    project_map: &HashMap<String, String>,
) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;
    let session_id = stringish(raw.session_id.as_ref(), &["sessionId", "id"]).or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
    })?;

    let message_count = raw
        .messages
        .iter()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .count();

    if message_count == 0 {
        return None;
    }

    let started_at = gemini_timestamp(raw.start_time.as_ref())
        .or_else(|| first_message_timestamp(&raw.messages))?;
    let ended_at = gemini_timestamp(raw.last_updated.as_ref())
        .or_else(|| last_message_timestamp(&raw.messages));

    let project_path = project_map.get(project_slug).map(PathBuf::from);

    let summary = raw.messages.iter().find_map(|m| {
        if raw_role(m.msg_type.as_ref()) == Some(Role::User) {
            extract_user_text(m).map(|t| t.chars().take(80).collect())
        } else {
            None
        }
    });

    let model = raw
        .messages
        .iter()
        .find_map(|m| stringish(m.model.as_ref(), &["model", "id", "name"]));

    let (input_total, output_total) = raw.messages.iter().fold((0u64, 0u64), |(inp, out), m| {
        if let Some(ref tokens) = m.tokens {
            (
                inp + value_u64(tokens.input.as_ref()).unwrap_or(0),
                out + value_u64(tokens.output.as_ref()).unwrap_or(0),
            )
        } else {
            (inp, out)
        }
    });

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::GeminiCli,
        project_path,
        project_name: Some(project_slug.to_string()),
        git_branch: None,
        started_at,
        ended_at,
        summary,
        model,
        token_usage: nonzero_token_usage(input_total, output_total, None, None),
        message_count,
        source_path: path.to_path_buf(),
    })
}

fn first_message_timestamp(messages: &[RawMessage]) -> Option<DateTime<Utc>> {
    messages
        .iter()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .find_map(|m| gemini_timestamp(m.timestamp.as_ref()))
}

fn last_message_timestamp(messages: &[RawMessage]) -> Option<DateTime<Utc>> {
    messages
        .iter()
        .rev()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .find_map(|m| gemini_timestamp(m.timestamp.as_ref()))
}

fn extract_user_text(msg: &RawMessage) -> Option<String> {
    match &msg.content {
        RawContent::Text(s) => Some(s.clone()),
        RawContent::Parts(parts) => {
            let text = if let Some(ref display_content) = msg.display_content {
                text_parts(display_content)
            } else {
                text_parts(parts)
            };
            (!text.is_empty()).then_some(text)
        }
        RawContent::Json(value) => Some(string_or_object_field_or_pretty(
            value,
            &["text", "content", "message"],
        ))
        .filter(|s| !s.is_empty()),
    }
}
