use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
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

    if let Some(home) = super::home_dir() {
        // OpenCode commonly stores data at ~/.local/share/opencode/storage/
        // even on Windows, so always check this path
        let local_share = home.join(".local").join("share").join("opencode").join("storage");
        result.push(local_share);
    }

    if std::env::var("AGHIST_HOME").is_err() {
        // Also check platform-native data directories
        if let Some(data_dir) =
            directories::ProjectDirs::from("", "", "opencode").map(|d| d.data_dir().to_path_buf())
        {
            let storage = data_dir.join("storage");
            if !result.iter().any(|p| p == &storage) {
                result.push(storage);
            }
        }

        if let Some(base) = directories::BaseDirs::new() {
            let appdata_path = base.data_dir().join("opencode");
            if !result.iter().any(|p| p == &appdata_path) {
                result.push(appdata_path);
            }
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
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let Ok(files) = std::fs::read_dir(project_entry.path()) else { continue };

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
        let part_dir = session.source_path.join("part");
        tracing::debug!(message_dir = %message_dir.display(), "loading OpenCode messages");
        if !message_dir.exists() {
            tracing::warn!(message_dir = %message_dir.display(), "message directory does not exist");
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        let mut file_count: usize = 0;
        let mut parse_failures: usize = 0;
        let files = std::fs::read_dir(&message_dir)?;

        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            file_count += 1;

            if let Some(msg) = parse_message_file(&path, &part_dir) {
                messages.push(msg);
            } else {
                parse_failures += 1;
                tracing::warn!(path = %path.display(), "failed to parse message file");
            }
        }

        messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        tracing::info!(
            message_dir = %message_dir.display(),
            files = file_count,
            parse_failures,
            messages = messages.len(),
            "OpenCode message loading complete"
        );
        Ok(messages)
    }
}

fn millis_to_datetime(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}

fn build_session_from_file(path: &Path, storage_base: &Path) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;

    // Try new format (time.created as millis) first, then legacy (createdAt as ISO string)
    let started_at = raw
        .time
        .as_ref()
        .and_then(|t| t.created)
        .and_then(millis_to_datetime)
        .or_else(|| {
            raw.created_at
                .as_deref()
                .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
        })?;

    let ended_at = raw
        .time
        .as_ref()
        .and_then(|t| t.updated)
        .and_then(millis_to_datetime)
        .or_else(|| {
            raw.updated_at
                .as_deref()
                .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
        });

    // New format uses "directory", legacy uses "cwd"
    let project_path = raw
        .directory
        .or(raw.cwd)
        .map(PathBuf::from);
    let project_name = project_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(String::from);

    // Extract model from new format
    let model = raw.model.and_then(|m| m.model_id);

    // Count messages in the message directory
    let message_dir = storage_base.join("message").join(&raw.id);
    let message_count = if message_dir.exists() {
        std::fs::read_dir(&message_dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
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
        model,
        token_usage: None,
        message_count,
        source_path: storage_base.to_path_buf(),
    })
}

fn parse_message_file(path: &Path, part_dir: &Path) -> Option<Message> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawMessage = serde_json::from_str(&data).ok()?;

    let role = match raw.role.as_deref() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return None,
    };

    // Try new format (time.created as millis) first, then legacy (timestamp as ISO string)
    let timestamp = raw
        .time
        .as_ref()
        .and_then(|t| t.created)
        .and_then(millis_to_datetime)
        .or_else(|| {
            raw.timestamp
                .as_deref()
                .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
        })
        .unwrap_or_else(Utc::now);

    let msg_id = raw.id.clone().unwrap_or_default();
    let mut content = Vec::new();

    // Try loading parts from part/{messageID}/ directory (new format)
    let msg_part_dir = part_dir.join(&msg_id);
    if msg_part_dir.exists() {
        load_parts_into_content(&msg_part_dir, &mut content);
    }

    // Fall back to legacy fields if no parts found
    if content.is_empty() {
        if let Some(text) = &raw.content {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(text));
            }
        }

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
    }

    // If still no content, try summary.title (new format user messages)
    if content.is_empty() {
        if let Some(ref summary) = raw.summary {
            if let Some(ref title) = summary.title {
                if !title.is_empty() {
                    content.push(ContentBlock::Text(title.clone()));
                }
            }
        }
    }

    if content.is_empty() {
        return None;
    }

    let token_usage = raw.tokens.as_ref().map(|t| TokenUsage {
        input_tokens: t.input.unwrap_or(0),
        output_tokens: t.output.unwrap_or(0),
        cache_read_tokens: t.cache.as_ref().and_then(|c| c.read),
        cache_write_tokens: t.cache.as_ref().and_then(|c| c.write),
    });

    let model = raw.model.and_then(|m| m.model_id);

    Some(Message {
        id: MessageId(msg_id),
        role,
        timestamp,
        content,
        model,
        token_usage,
    })
}

/// Load content blocks from part files in a message's part directory.
fn load_parts_into_content(part_dir: &Path, content: &mut Vec<ContentBlock>) {
    let Ok(entries) = std::fs::read_dir(part_dir) else {
        return;
    };

    let mut parts: Vec<(String, RawPart)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let Ok(data) = std::fs::read_to_string(&path) else { continue };
        let Ok(part) = serde_json::from_str::<RawPart>(&data) else { continue };

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        parts.push((filename, part));
    }

    // Sort by filename (part IDs are roughly chronological)
    parts.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, part) in &parts {
        match part.part_type.as_str() {
            "text" => {
                if let Some(ref text) = part.text {
                    if !text.is_empty() {
                        content.extend(parse_text_with_code_blocks(text));
                    }
                }
            }
            "tool" => {
                let tool_name = part.tool.clone().unwrap_or_else(|| "unknown".to_string());
                let call_id = part.call_id.clone().unwrap_or_default();
                let arguments = part
                    .state
                    .as_ref()
                    .and_then(|s| s.input.as_ref())
                    .map(|input| serde_json::to_string_pretty(input).unwrap_or_default())
                    .unwrap_or_default();
                content.push(ContentBlock::ToolUse(ToolCall {
                    id: call_id,
                    name: tool_name,
                    arguments,
                }));

                // Include tool output as a result
                if let Some(ref state) = part.state {
                    if let Some(ref output) = state.output {
                        if !output.is_empty() {
                            let tool_call_id = part.call_id.clone().unwrap_or_default();
                            let success = state.status.as_deref() == Some("completed");
                            content.push(ContentBlock::ToolResult(
                                crate::model::ToolResult {
                                    tool_call_id,
                                    success,
                                    output: output.clone(),
                                },
                            ));
                        }
                    }
                }
            }
            // Skip step-start, step-finish, and other structural types
            _ => {}
        }
    }
}

// -- Raw deserialization types --

#[derive(Deserialize)]
struct RawSession {
    id: String,
    title: Option<String>,
    /// New format: "directory"
    directory: Option<String>,
    /// Legacy format: "cwd"
    cwd: Option<String>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: ISO timestamp strings
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    /// New format: model info
    model: Option<RawModel>,
}

#[derive(Deserialize)]
struct RawTime {
    created: Option<i64>,
    updated: Option<i64>,
    #[allow(dead_code)]
    completed: Option<i64>,
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(rename = "modelID")]
    model_id: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    role: Option<String>,
    /// Legacy format: ISO timestamp
    timestamp: Option<String>,
    /// New format: nested time object with millis
    time: Option<RawTime>,
    /// Legacy format: text content
    content: Option<String>,
    /// Legacy format: code changes
    #[serde(rename = "codeChanges")]
    code_changes: Option<Vec<RawCodeChange>>,
    /// New format: summary with title and diffs
    summary: Option<RawSummary>,
    /// New format: token usage
    tokens: Option<RawTokens>,
    /// New format: model info
    model: Option<RawModel>,
}

#[derive(Deserialize)]
struct RawSummary {
    title: Option<String>,
}

#[allow(clippy::struct_field_names)]
#[derive(Deserialize)]
struct RawTokens {
    input: Option<u64>,
    output: Option<u64>,
    cache: Option<RawCache>,
}

#[derive(Deserialize)]
struct RawCache {
    read: Option<u64>,
    write: Option<u64>,
}

#[derive(Deserialize)]
struct RawCodeChange {
    path: Option<String>,
    diff: Option<String>,
}

#[derive(Deserialize)]
struct RawPart {
    #[serde(rename = "type")]
    part_type: String,
    /// Text content (for type="text")
    text: Option<String>,
    /// Tool name (for type="tool")
    tool: Option<String>,
    /// Tool call ID (for type="tool")
    #[serde(rename = "callID")]
    call_id: Option<String>,
    /// Tool state with input/output (for type="tool")
    state: Option<RawToolState>,
}

#[derive(Deserialize)]
struct RawToolState {
    status: Option<String>,
    input: Option<serde_json::Value>,
    output: Option<String>,
}
