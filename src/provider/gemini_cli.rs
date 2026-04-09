use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

pub struct GeminiCliProvider {
    dirs: Vec<PathBuf>,
}

impl GeminiCliProvider {
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
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
        result.push(home.join(".gemini"));
    }
    result
}

impl HistoryProvider for GeminiCliProvider {
    fn provider(&self) -> Provider {
        Provider::GeminiCli
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            // Load project name mapping
            let project_map = load_project_map(base);

            // Scan tmp/{project}/chats/session-*.json
            let tmp_dir = base.join("tmp");
            if !tmp_dir.exists() {
                continue;
            }

            let project_dirs = std::fs::read_dir(&tmp_dir).map_err(|e| {
                ProviderError::Discovery {
                    provider: "Gemini CLI",
                    source: e,
                }
            })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let project_slug = project_entry
                    .file_name()
                    .to_string_lossy()
                    .to_string();

                let chats_dir = project_entry.path().join("chats");
                if !chats_dir.exists() {
                    continue;
                }

                let chat_files = std::fs::read_dir(&chats_dir).map_err(|e| {
                    ProviderError::Discovery {
                        provider: "Gemini CLI",
                        source: e,
                    }
                })?;

                for file_entry in chat_files.flatten() {
                    let path = file_entry.path();
                    let fname = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    if !fname.starts_with("session-")
                        || !std::path::Path::new(fname)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                    {
                        continue;
                    }

                    if let Some(session) = build_session_from_file(
                        &path,
                        &project_slug,
                        &project_map,
                    ) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        let data = std::fs::read_to_string(&session.source_path)?;
        let raw: RawSession = serde_json::from_str(&data)?;
        Ok(convert_messages(&raw.messages))
    }
}

#[derive(Deserialize)]
struct ProjectsFile {
    projects: HashMap<String, String>,
}

fn load_project_map(base: &Path) -> HashMap<String, String> {
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

fn build_session_from_file(
    path: &Path,
    project_slug: &str,
    project_map: &HashMap<String, String>,
) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;

    let message_count = raw
        .messages
        .iter()
        .filter(|m| m.msg_type == "user" || m.msg_type == "gemini")
        .count();

    if message_count == 0 {
        return None;
    }

    let started_at = raw.start_time.parse::<DateTime<Utc>>().ok()?;
    let ended_at = raw.last_updated.parse::<DateTime<Utc>>().ok();

    let project_path = project_map.get(project_slug).map(PathBuf::from);

    // Get first user message as summary
    let summary = raw.messages.iter().find_map(|m| {
        if m.msg_type == "user" {
            extract_user_text(m).map(|t| t.chars().take(80).collect())
        } else {
            None
        }
    });

    // Get model from first gemini message
    let model = raw
        .messages
        .iter()
        .find_map(|m| m.model.clone());

    // Sum tokens
    let (input_total, output_total) = raw.messages.iter().fold((0u64, 0u64), |(inp, out), m| {
        if let Some(ref tokens) = m.tokens {
            (inp + tokens.input.unwrap_or(0), out + tokens.output.unwrap_or(0))
        } else {
            (inp, out)
        }
    });

    let token_usage = if input_total > 0 || output_total > 0 {
        Some(TokenUsage {
            input_tokens: input_total,
            output_tokens: output_total,
            cache_read_tokens: None,
            cache_write_tokens: None,
        })
    } else {
        None
    };

    Some(Session {
        id: SessionId(raw.session_id),
        provider: Provider::GeminiCli,
        project_path,
        project_name: Some(project_slug.to_string()),
        git_branch: None,
        started_at,
        ended_at,
        summary,
        model,
        token_usage,
        message_count,
        source_path: path.to_path_buf(),
    })
}

fn extract_user_text(msg: &RawMessage) -> Option<String> {
    match &msg.content {
        RawContent::Text(s) => Some(s.clone()),
        RawContent::Parts(parts) => {
            // Use displayContent if available, otherwise first text part
            if let Some(ref dc) = msg.display_content {
                dc.iter().find_map(|p| p.text.clone())
            } else {
                parts.iter().find_map(|p| p.text.clone())
            }
        }
    }
}

fn convert_messages(raw_messages: &[RawMessage]) -> Vec<Message> {
    let mut messages = Vec::new();

    for msg in raw_messages {
        let role = match msg.msg_type.as_str() {
            "user" => Role::User,
            "gemini" => Role::Assistant,
            _ => continue,
        };

        let timestamp = msg
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let mut content = Vec::new();

        // Extract text content
        let text = match &msg.content {
            RawContent::Text(s) => Some(s.clone()),
            RawContent::Parts(parts) => {
                if role == Role::User {
                    // For user messages, prefer displayContent
                    if let Some(ref dc) = msg.display_content {
                        Some(dc.iter().filter_map(|p| p.text.as_ref()).cloned().collect::<Vec<_>>().join("\n"))
                    } else {
                        Some(parts.iter().filter_map(|p| p.text.as_ref()).cloned().collect::<Vec<_>>().join("\n"))
                    }
                } else {
                    Some(parts.iter().filter_map(|p| p.text.as_ref()).cloned().collect::<Vec<_>>().join("\n"))
                }
            }
        };

        if let Some(text) = text {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(&text));
            }
        }

        // Thinking
        if let Some(ref thoughts) = msg.thoughts {
            for thought in thoughts {
                let desc = thought.description.as_deref().unwrap_or("");
                if !desc.is_empty() {
                    content.push(ContentBlock::Thinking(desc.to_string()));
                }
            }
        }

        // Tool calls
        if let Some(ref tool_calls) = msg.tool_calls {
            for tc in tool_calls {
                content.push(ContentBlock::ToolUse(ToolCall {
                    id: tc.id.clone().unwrap_or_default(),
                    name: tc.name.clone().unwrap_or_else(|| "unknown".to_string()),
                    arguments: tc
                        .args
                        .as_ref()
                        .map(|a| serde_json::to_string_pretty(a).unwrap_or_default())
                        .unwrap_or_default(),
                }));
            }
        }

        if content.is_empty() {
            continue;
        }

        let token_usage = msg.tokens.as_ref().map(|t| TokenUsage {
            input_tokens: t.input.unwrap_or(0),
            output_tokens: t.output.unwrap_or(0),
            cache_read_tokens: t.cached,
            cache_write_tokens: None,
        });

        messages.push(Message {
            id: MessageId(msg.id.clone().unwrap_or_default()),
            role,
            timestamp,
            content,
            model: msg.model.clone(),
            token_usage,
        });
    }

    messages
}

// -- Raw deserialization types --

#[derive(Deserialize)]
struct RawSession {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "lastUpdated")]
    last_updated: String,
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    content: RawContent,
    #[serde(rename = "displayContent")]
    display_content: Option<Vec<TextPart>>,
    thoughts: Option<Vec<Thought>>,
    tokens: Option<RawTokens>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<RawToolCall>>,
    model: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Text(String),
    Parts(Vec<TextPart>),
}

impl Default for RawContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

#[derive(Deserialize)]
struct TextPart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct Thought {
    description: Option<String>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
}

#[derive(Deserialize)]
struct RawToolCall {
    id: Option<String>,
    name: Option<String>,
    args: Option<serde_json::Value>,
}
