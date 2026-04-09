use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

pub struct OpenCodeProvider {
    dirs: Vec<PathBuf>,
}

impl OpenCodeProvider {
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

    // Linux: ~/.local/share/opencode/storage/
    if let Some(data_dir) =
        directories::ProjectDirs::from("", "", "opencode").map(|d| d.data_dir().to_path_buf())
    {
        result.push(data_dir.join("storage"));
    }

    // Windows: %APPDATA%\opencode\
    if let Some(base) = directories::BaseDirs::new() {
        let appdata_path = base.data_dir().join("opencode");
        if !result.iter().any(|p| p == &appdata_path) {
            result.push(appdata_path);
        }
    }

    // OPENCODE_DATA_DIR env var
    if let Ok(data_dir) = std::env::var("OPENCODE_DATA_DIR") {
        result.push(PathBuf::from(data_dir));
    }

    result
}

impl HistoryProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        Provider::OpenCode
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

            // Scan session/{projectHash}/*.json
            let session_dir = base.join("session");
            if !session_dir.exists() {
                continue;
            }

            let project_dirs =
                std::fs::read_dir(&session_dir).map_err(|e| ProviderError::Discovery {
                    provider: "OpenCode",
                    source: e,
                })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().map_or(false, |t| t.is_dir()) {
                    continue;
                }

                let files =
                    match std::fs::read_dir(project_entry.path()) {
                        Ok(entries) => entries,
                        Err(_) => continue,
                    };

                for file_entry in files.flatten() {
                    let path = file_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }

                    if let Some(session) = build_session_from_file(&path, base) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        // source_path points to the storage base dir, session id is in session.id
        // Messages are in message/{sessionID}/msg_*.json
        let message_dir = session.source_path.join("message").join(&session.id.0);
        if !message_dir.exists() {
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        let files = std::fs::read_dir(&message_dir)?;

        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            if let Some(msg) = parse_message_file(&path) {
                messages.push(msg);
            }
        }

        messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(messages)
    }
}

fn build_session_from_file(path: &Path, storage_base: &Path) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;

    let started_at = raw
        .created_at
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())?;

    let ended_at = raw
        .updated_at
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok());

    let project_path = raw.cwd.map(PathBuf::from);
    let project_name = project_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(String::from);

    // Count messages in the message directory
    let message_dir = storage_base.join("message").join(&raw.id);
    let message_count = if message_dir.exists() {
        std::fs::read_dir(&message_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            == Some("json")
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Some(Session {
        id: SessionId(raw.id),
        provider: Provider::OpenCode,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw.title,
        model: None,
        token_usage: None,
        message_count,
        source_path: storage_base.to_path_buf(),
    })
}

fn parse_message_file(path: &Path) -> Option<Message> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawMessage = serde_json::from_str(&data).ok()?;

    let role = match raw.role.as_deref() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return None,
    };

    let timestamp = raw
        .timestamp
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);

    let mut content = Vec::new();

    if let Some(text) = &raw.content {
        if !text.is_empty() {
            content.extend(parse_text_with_code_blocks(text));
        }
    }

    // Code changes as separate blocks
    if let Some(changes) = &raw.code_changes {
        for change in changes {
            let label = change.path.as_deref().unwrap_or("diff");
            let diff = change.diff.as_deref().unwrap_or("");
            if !diff.is_empty() {
                content.push(ContentBlock::CodeBlock {
                    language: Some(format!("diff ({label})")),
                    code: diff.to_string(),
                });
            }
        }
    }

    if content.is_empty() {
        return None;
    }

    Some(Message {
        id: MessageId(raw.id.unwrap_or_default()),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    })
}

#[derive(Deserialize)]
struct RawSession {
    id: String,
    title: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    role: Option<String>,
    timestamp: Option<String>,
    content: Option<String>,
    #[serde(rename = "codeChanges")]
    code_changes: Option<Vec<RawCodeChange>>,
}

#[derive(Deserialize)]
struct RawCodeChange {
    path: Option<String>,
    diff: Option<String>,
}
