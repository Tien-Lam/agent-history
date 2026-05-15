use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, ToolCall,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

pub struct CodexCliProvider {
    dirs: Vec<PathBuf>,
}

impl CodexCliProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(home) = super::home_dir() {
        result.push(home.join(".codex").join("sessions"));
    }
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        result.push(PathBuf::from(codex_home).join("sessions"));
    }
    result
}

impl HistoryProvider for CodexCliProvider {
    fn provider(&self) -> Provider {
        Provider::CodexCli
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            if !base.exists() {
                continue;
            }

            // Scan {YYYY}/{MM}/{DD}/rollout-*.jsonl
            collect_rollout_files(base, &mut sessions);
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_rollout_messages(&session.source_path)
    }
}

fn collect_rollout_files(base: &Path, sessions: &mut Vec<Session>) {
    // Walk year/month/day directories
    let Ok(years) = std::fs::read_dir(base) else {
        return;
    };

    for year_entry in years.flatten() {
        if !year_entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }

        let Ok(months) = std::fs::read_dir(year_entry.path()) else {
            continue;
        };

        for month_entry in months.flatten() {
            if !month_entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }

            let Ok(days) = std::fs::read_dir(month_entry.path()) else {
                continue;
            };

            for day_entry in days.flatten() {
                if !day_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let Ok(files) = std::fs::read_dir(day_entry.path()) else {
                    continue;
                };

                for file_entry in files.flatten() {
                    let path = file_entry.path();
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if fname.starts_with("rollout-")
                        && std::path::Path::new(fname)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                    {
                        if let Some(session) = build_session_from_rollout(&path) {
                            sessions.push(session);
                        }
                    }
                }
            }
        }
    }
}

fn build_session_from_rollout(path: &Path) -> Option<Session> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut first_user_message: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }

        let entry: RawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if let Some(ts) = &entry.timestamp {
            if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                if first_timestamp.is_none() {
                    first_timestamp = Some(dt);
                }
                last_timestamp = Some(dt);
            }
        }

        match entry.entry_type.as_deref() {
            Some("user" | "assistant") => {
                message_count += 1;
                if entry.entry_type.as_deref() == Some("user") && first_user_message.is_none() {
                    first_user_message = entry.content.map(|c| c.chars().take(80).collect());
                }
            }
            Some("event_msg") => {
                // Newer Codex format
                if let Some(ref payload) = entry.payload {
                    if let Some("user_message" | "agent_message") = payload.entry_type.as_deref() {
                        message_count += 1;
                        if payload.entry_type.as_deref() == Some("user_message")
                            && first_user_message.is_none()
                        {
                            first_user_message = payload
                                .message
                                .as_ref()
                                .map(|m| m.chars().take(80).collect());
                        }
                    }
                }
            }
            _ => {}
        }
    }

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

fn parse_rollout_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Codex CLI messages");
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut line_count: usize = 0;
    let mut parse_errors: usize = 0;
    let mut skipped_types: usize = 0;
    let mut empty_content: usize = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        line_count += 1;

        let entry: RawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                parse_errors += 1;
                tracing::warn!(line_num = line_count, error = %e, "failed to parse JSONL line");
                continue;
            }
        };

        let entry_type = entry.entry_type.as_deref().unwrap_or("");
        let role = match entry_type {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool_use" => Role::Tool,
            "error" => {
                if let Some(error_msg) = entry.error.as_deref() {
                    messages.push(error_message(
                        entry_timestamp(&entry),
                        error_msg.to_string(),
                    ));
                }
                continue;
            }
            "event_msg" => {
                push_event_msg(&mut messages, &entry);
                continue;
            }
            "response_item" => {
                push_response_item(&mut messages, &entry);
                continue;
            }
            _ => {
                skipped_types += 1;
                tracing::trace!(entry_type, "skipping non-message entry");
                continue;
            }
        };

        let timestamp = entry_timestamp(&entry);
        let content = legacy_content(&entry, role);

        if content.is_empty() {
            empty_content += 1;
            tracing::trace!(entry_type, "skipping entry with empty content");
            continue;
        }

        messages.push(message(role, timestamp, content));
    }

    tracing::info!(
        path = %path.display(),
        lines = line_count,
        parse_errors,
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Codex CLI message loading complete"
    );

    Ok(messages)
}

fn entry_timestamp(entry: &RawEntry) -> DateTime<Utc> {
    entry
        .timestamp
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now)
}

fn message(role: Role, timestamp: DateTime<Utc>, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(String::new()),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    }
}

fn error_message(timestamp: DateTime<Utc>, error_msg: String) -> Message {
    message(
        Role::System,
        timestamp,
        vec![ContentBlock::Error(error_msg)],
    )
}

fn push_text_message(
    messages: &mut Vec<Message>,
    role: Role,
    timestamp: DateTime<Utc>,
    text: &str,
) {
    if !text.is_empty() {
        messages.push(message(role, timestamp, parse_text_with_code_blocks(text)));
    }
}

fn push_event_msg(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = payload.entry_type.as_deref().unwrap_or("");
    let timestamp = entry_timestamp(entry);
    match payload_type {
        "user_message" => {
            if let Some(msg_text) = payload.message.as_deref() {
                push_text_message(messages, Role::User, timestamp, msg_text);
            }
        }
        "agent_message" => {
            if let Some(msg_text) = payload.message.as_deref() {
                push_text_message(messages, Role::Assistant, timestamp, msg_text);
            }
        }
        _ => tracing::trace!(payload_type, "skipping event_msg"),
    }
}

fn push_response_item(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = payload.entry_type.as_deref().unwrap_or("");
    let timestamp = entry_timestamp(entry);
    match payload_type {
        "function_call" => messages.push(message(
            Role::Tool,
            timestamp,
            vec![ContentBlock::ToolUse(ToolCall {
                id: payload.call_id.clone().unwrap_or_default(),
                name: payload
                    .name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                arguments: payload.arguments.clone().unwrap_or_default(),
            })],
        )),
        "function_call_output" => {
            let output = payload.output.clone().unwrap_or_default();
            if !output.is_empty() {
                messages.push(message(
                    Role::Tool,
                    timestamp,
                    vec![ContentBlock::ToolResult(crate::model::ToolResult {
                        tool_call_id: payload.call_id.clone().unwrap_or_default(),
                        success: true,
                        output,
                    })],
                ));
            }
        }
        _ => tracing::trace!(payload_type, "skipping response_item"),
    }
}

fn legacy_content(entry: &RawEntry, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    let Some(text) = entry.content.as_ref().filter(|text| !text.is_empty()) else {
        return content;
    };
    if role == Role::Tool {
        content.push(ContentBlock::ToolUse(ToolCall {
            id: String::new(),
            name: text.clone(),
            arguments: entry
                .tool_calls
                .as_ref()
                .map(|tc| serde_json::to_string_pretty(tc).unwrap_or_default())
                .unwrap_or_default(),
        }));
    } else {
        content.extend(parse_text_with_code_blocks(text));
    }
    content
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    content: Option<String>,
    timestamp: Option<String>,
    tool_calls: Option<serde_json::Value>,
    error: Option<String>,
    /// Newer Codex format wraps messages in a payload object
    payload: Option<RawPayload>,
}

#[derive(Deserialize)]
struct RawPayload {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    /// `event_msg`: user/agent message text
    message: Option<String>,
    /// `response_item` `function_call`: tool name
    name: Option<String>,
    /// `response_item` `function_call`: call ID
    call_id: Option<String>,
    /// `response_item` `function_call`: arguments as JSON string
    arguments: Option<String>,
    /// `response_item` `function_call_output`: output text
    output: Option<String>,
}
