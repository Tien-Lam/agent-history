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
                if !entry.file_type().map_or(false, |t| t.is_dir()) {
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
    let workspace: WorkspaceYaml = serde_yaml::from_str(&yaml_content).ok()?;

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
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| {
            l.contains("\"user.message\"")
                || l.contains("\"assistant.message\"")
        })
        .count()
}

fn parse_events_jsonl(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let event: RawEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let role = match event.event_type.as_deref() {
            Some(t) if t.contains("user") => Role::User,
            Some(t) if t.contains("assistant.message") => Role::Assistant,
            Some(t) if t.contains("tool") => Role::Tool,
            _ => continue,
        };

        let timestamp = event
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let mut content = Vec::new();

        if let Some(text) = &event.content {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(text));
            }
        }

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
        }

        if content.is_empty() {
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
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<u64>,
}
