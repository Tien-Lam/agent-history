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
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
        result.push(home.join(".codex").join("sessions"));
    }
    // Also check CODEX_HOME env var
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
            collect_rollout_files(base, &mut sessions)?;
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_rollout_messages(&session.source_path)
    }
}

fn collect_rollout_files(
    base: &Path,
    sessions: &mut Vec<Session>,
) -> Result<(), ProviderError> {
    // Walk year/month/day directories
    let years = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for year_entry in years.flatten() {
        if !year_entry.file_type().map_or(false, |t| t.is_dir()) {
            continue;
        }

        let months = match std::fs::read_dir(year_entry.path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for month_entry in months.flatten() {
            if !month_entry.file_type().map_or(false, |t| t.is_dir()) {
                continue;
            }

            let days = match std::fs::read_dir(month_entry.path()) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for day_entry in days.flatten() {
                if !day_entry.file_type().map_or(false, |t| t.is_dir()) {
                    continue;
                }

                let files = match std::fs::read_dir(day_entry.path()) {
                    Ok(entries) => entries,
                    Err(_) => continue,
                };

                for file_entry in files.flatten() {
                    let path = file_entry.path();
                    let fname = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    if fname.starts_with("rollout-") && fname.ends_with(".jsonl") {
                        if let Some(session) = build_session_from_rollout(&path) {
                            sessions.push(session);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_session_from_rollout(path: &Path) -> Option<Session> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut first_user_message: Option<String> = None;

    for line in reader.lines().flatten() {
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
            Some("user") | Some("assistant") => {
                message_count += 1;
                if entry.entry_type.as_deref() == Some("user")
                    && first_user_message.is_none()
                {
                    first_user_message =
                        entry.content.map(|c| c.chars().take(80).collect());
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
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: RawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let role = match entry.entry_type.as_deref() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            Some("tool_use") => Role::Tool,
            Some("error") => {
                if let Some(error_msg) = entry.error {
                    messages.push(Message {
                        id: MessageId(String::new()),
                        role: Role::System,
                        timestamp: entry
                            .timestamp
                            .as_deref()
                            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
                            .unwrap_or_else(Utc::now),
                        content: vec![ContentBlock::Error(error_msg)],
                        model: None,
                        token_usage: None,
                    });
                }
                continue;
            }
            _ => continue,
        };

        let timestamp = entry
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let mut content = Vec::new();

        if let Some(text) = &entry.content {
            if !text.is_empty() {
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
            }
        }

        if content.is_empty() {
            continue;
        }

        messages.push(Message {
            id: MessageId(String::new()),
            role,
            timestamp,
            content,
            model: None,
            token_usage: None,
        });
    }

    Ok(messages)
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    content: Option<String>,
    timestamp: Option<String>,
    tool_calls: Option<serde_json::Value>,
    error: Option<String>,
}
