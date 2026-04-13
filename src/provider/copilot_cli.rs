use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

pub struct CopilotCliProvider {
    dirs: Vec<PathBuf>,
}

impl CopilotCliProvider {
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
        result.push(home.join(".copilot").join("session-state"));
    }
    result
}

impl HistoryProvider for CopilotCliProvider {
    fn provider(&self) -> Provider {
        Provider::CopilotCli
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

            let session_dirs = std::fs::read_dir(base).map_err(|e| ProviderError::Discovery {
                provider: "Copilot CLI",
                source: e,
            })?;

            for entry in session_dirs.flatten() {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let session_dir = entry.path();
                let workspace_path = session_dir.join("workspace.yaml");

                if !workspace_path.exists() {
                    continue;
                }

                if let Some(session) = build_session(&session_dir, &workspace_path) {
                    sessions.push(session);
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        // Look for events.jsonl in the session directory
        let events_path = session.source_path.join("events.jsonl");
        if events_path.exists() {
            parse_events_jsonl(&events_path)
        } else {
            // Fall back to checkpoint markdown
            let checkpoint_path = session.source_path.join("checkpoints").join("index.md");
            if checkpoint_path.exists() {
                parse_checkpoint_md(&checkpoint_path)
            } else {
                Ok(Vec::new())
            }
        }
    }
}

#[derive(Deserialize)]
struct WorkspaceYaml {
    id: Option<String>,
    cwd: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

fn build_session(session_dir: &Path, workspace_path: &Path) -> Option<Session> {
    let yaml_content = std::fs::read_to_string(workspace_path).ok()?;
    let workspace: WorkspaceYaml = serde_yaml_ng::from_str(&yaml_content).ok()?;

    let session_id = workspace.id.or_else(|| {
        session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    })?;

    let started_at = workspace
        .created_at
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())?;

    let ended_at = workspace
        .updated_at
        .as_deref()
        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok());

    let project_path = workspace.cwd.map(PathBuf::from);
    let project_name = project_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(String::from);

    // Count events to estimate message count
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
    let Ok(file) = std::fs::File::open(path) else { return 0 };
    let reader = BufReader::new(file);
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| {
            l.contains("\"user.message\"")
                || l.contains("\"assistant.message\"")
        })
        .count()
}

#[allow(clippy::too_many_lines)]
fn parse_events_jsonl(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Copilot CLI messages");
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

        let event: RawEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                parse_errors += 1;
                tracing::warn!(line_num = line_count, error = %e, "failed to parse JSONL line");
                continue;
            }
        };

        let event_type_str = event.event_type.as_deref().unwrap_or("");

        let role = match event_type_str {
            t if t.contains("user") => Role::User,
            t if t.contains("assistant.message") => Role::Assistant,
            t if t.contains("tool") => Role::Tool,
            _ => {
                skipped_types += 1;
                tracing::trace!(event_type = event_type_str, "skipping non-message event");
                continue;
            }
        };

        let timestamp = event
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let mut content = Vec::new();

        // Try top-level content first, then data.content (newer format)
        let text = event.content.as_deref()
            .or_else(|| event.data.as_ref().and_then(|d| d.content.as_deref()));
        if let Some(text) = text {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(text));
            }
        }

        // Try top-level tool fields first, then data.toolRequests (newer format)
        if let Some(tool_name) = &event.tool_name {
            content.push(ContentBlock::ToolUse(ToolCall {
                id: event.tool_call_id.clone().unwrap_or_default(),
                name: tool_name.clone(),
                arguments: event
                    .tool_args
                    .as_ref()
                    .map(|a| serde_json::to_string_pretty(a).unwrap_or_default())
                    .unwrap_or_default(),
            }));
        } else if let Some(ref data) = event.data {
            if let Some(ref tool_requests) = data.tool_requests {
                for tr in tool_requests {
                    content.push(ContentBlock::ToolUse(ToolCall {
                        id: tr.tool_call_id.clone().unwrap_or_default(),
                        name: tr.name.clone().unwrap_or_else(|| "unknown".to_string()),
                        arguments: tr
                            .arguments
                            .as_ref()
                            .map(|a| serde_json::to_string_pretty(a).unwrap_or_default())
                            .unwrap_or_default(),
                    }));
                }
            }
        }

        if content.is_empty() {
            empty_content += 1;
            tracing::debug!(
                event_type = event_type_str,
                has_data = event.data.is_some(),
                data_has_content = event.data.as_ref().is_some_and(|d| d.content.is_some()),
                "skipping event with empty content"
            );
            continue;
        }

        let token_usage = event.usage.as_ref().map(|u| TokenUsage {
            input_tokens: u.input_tokens.unwrap_or(0),
            output_tokens: u.output_tokens.unwrap_or(0),
            cache_read_tokens: None,
            cache_write_tokens: None,
        });

        messages.push(Message {
            id: MessageId(event.id.unwrap_or_default()),
            role,
            timestamp,
            content,
            model: event.model,
            token_usage,
        });
    }

    tracing::info!(
        path = %path.display(),
        lines = line_count,
        parse_errors,
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Copilot CLI message loading complete"
    );

    Ok(messages)
}

fn parse_checkpoint_md(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty()
        || content.lines().all(|l| l.starts_with('#') || l.starts_with('|') || l.trim().is_empty())
    {
        return Ok(Vec::new());
    }

    Ok(vec![Message {
        id: MessageId("checkpoint".to_string()),
        role: Role::System,
        timestamp: Utc::now(),
        content: parse_text_with_code_blocks(&content),
        model: None,
        token_usage: None,
    }])
}

#[derive(Deserialize)]
struct RawEvent {
    id: Option<String>,
    #[serde(rename = "type")]
    event_type: Option<String>,
    timestamp: Option<String>,
    content: Option<String>,
    model: Option<String>,
    #[serde(rename = "toolName")]
    tool_name: Option<String>,
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    #[serde(rename = "toolArgs")]
    tool_args: Option<serde_json::Value>,
    usage: Option<RawUsage>,
    /// Newer Copilot format nests content inside a `data` object
    data: Option<RawEventData>,
}

#[derive(Deserialize)]
struct RawEventData {
    content: Option<String>,
    #[serde(rename = "toolRequests")]
    tool_requests: Option<Vec<RawToolRequest>>,
}

#[derive(Deserialize)]
struct RawToolRequest {
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    name: Option<String>,
    arguments: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<u64>,
}
