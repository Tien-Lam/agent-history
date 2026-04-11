use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
    ToolResult,
};

pub struct ClaudeCodeProvider {
    dirs: Vec<PathBuf>,
}

impl ClaudeCodeProvider {
    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }

    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(home) = super::home_dir() {
        result.push(home.join(".claude"));
    }
    result
}

fn projects_dir(base: &Path) -> PathBuf {
    base.join("projects")
}

impl HistoryProvider for ClaudeCodeProvider {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            let history_path = base.join("history.jsonl");
            if !history_path.exists() {
                continue;
            }

            // Build a map of sessionId -> history entries for metadata
            let history_entries = parse_history_index(&history_path)?;

            // Scan project directories for .jsonl session files
            let projects = projects_dir(base);
            if !projects.exists() {
                continue;
            }

            let project_dirs = std::fs::read_dir(&projects).map_err(|e| {
                ProviderError::Discovery {
                    provider: "Claude Code",
                    source: e,
                }
            })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let project_dir = project_entry.path();
                let project_name = decode_project_name(
                    project_entry.file_name().to_string_lossy().as_ref(),
                );

                let entries = std::fs::read_dir(&project_dir).map_err(|e| {
                    ProviderError::Discovery {
                        provider: "Claude Code",
                        source: e,
                    }
                })?;

                for file_entry in entries.flatten() {
                    let path = file_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }

                    let session_id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(session) = build_session_metadata(
                        &path,
                        &session_id,
                        &project_name,
                        &history_entries,
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
        parse_session_messages(&session.source_path)
    }
}

// -- History index parsing --

#[derive(Deserialize)]
struct HistoryEntry {
    display: Option<String>,
    timestamp: Option<u64>,
    #[allow(dead_code)]
    project: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

fn parse_history_index(path: &Path) -> Result<Vec<HistoryEntry>, ProviderError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
            entries.push(entry);
        }
    }

    Ok(entries)
}

/// Decode the project directory name back to a readable path.
/// Claude Code encodes `V:\Projects\agent-history` as `V--Projects-agent-history`.
/// The encoding is lossy (both `/` and literal `-` become `-`), so we only
/// decode `--` (drive separator) and leave single dashes as-is.
fn decode_project_name(encoded: &str) -> String {
    encoded.replace("--", ":/")
}

fn build_session_metadata(
    source_path: &Path,
    session_id: &str,
    project_name: &str,
    history_entries: &[HistoryEntry],
) -> Option<Session> {
    // Quick scan of the session file for timestamps and message count
    let file = std::fs::File::open(source_path).ok()?;
    let reader = BufReader::new(file);

    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }

        let entry: RawSessionEntry = match serde_json::from_str(&line) {
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

        if let Some("user" | "assistant") = entry.entry_type.as_deref() {
            message_count += 1;

            if git_branch.is_none() {
                if let Some(ref branch) = entry.git_branch {
                    git_branch = Some(branch.clone());
                }
            }
            if cwd.is_none() {
                if let Some(ref c) = entry.cwd {
                    cwd = Some(c.clone());
                }
            }

            if entry.entry_type.as_deref() == Some("assistant") {
                if let Some(ref msg) = entry.message {
                    if model.is_none() {
                        if let Some(ref m) = msg.model {
                            model = Some(m.clone());
                        }
                    }
                    if let Some(ref usage) = msg.usage {
                        total_input_tokens +=
                            usage.input_tokens.unwrap_or(0);
                        total_output_tokens +=
                            usage.output_tokens.unwrap_or(0);
                    }
                }
            }
        }
    }

    if message_count == 0 {
        return None;
    }

    // Get first user message as summary from history entries
    let summary = history_entries
        .iter()
        .find(|e| e.session_id.as_deref() == Some(session_id))
        .and_then(|e| e.display.clone());

    // Use history entry timestamp if we didn't find one in the session file
    let started_at = first_timestamp.or_else(|| {
        history_entries
            .iter()
            .find(|e| e.session_id.as_deref() == Some(session_id))
            .and_then(|e| e.timestamp)
            .and_then(|ts| Utc.timestamp_millis_opt(ts.cast_signed()).single())
    })?;

    let token_usage = if total_input_tokens > 0 || total_output_tokens > 0 {
        Some(TokenUsage {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
        })
    } else {
        None
    };

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

// -- Session message parsing --

#[derive(Deserialize)]
struct RawSessionEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    uuid: Option<String>,
    timestamp: Option<String>,
    message: Option<RawMessage>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<serde_json::Value>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[allow(clippy::struct_field_names)]
#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

fn parse_session_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: RawSessionEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let role = match entry.entry_type.as_deref() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => continue,
        };

        let Some(ref msg) = entry.message else { continue };

        let timestamp = entry
            .timestamp
            .as_deref()
            .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);

        let id = entry.uuid.unwrap_or_default();

        let content = parse_message_content(msg, role);
        if content.is_empty() {
            continue;
        }

        let token_usage = msg.usage.as_ref().map(|u| TokenUsage {
            input_tokens: u.input_tokens.unwrap_or(0),
            output_tokens: u.output_tokens.unwrap_or(0),
            cache_read_tokens: u.cache_read_input_tokens,
            cache_write_tokens: u.cache_creation_input_tokens,
        });

        messages.push(Message {
            id: MessageId(id),
            role,
            timestamp,
            content,
            model: msg.model.clone(),
            token_usage,
        });
    }

    Ok(messages)
}

fn parse_message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let Some(content) = &msg.content else {
        return Vec::new();
    };

    match content {
        serde_json::Value::String(s) => {
            parse_text_with_code_blocks(s)
        }
        serde_json::Value::Array(arr) => {
            let mut blocks = Vec::new();
            for item in arr {
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "text" => {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            blocks.extend(parse_text_with_code_blocks(text));
                        }
                    }
                    "thinking" => {
                        if let Some(text) = item.get("thinking").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Thinking(text.to_string()));
                            }
                        }
                    }
                    "tool_use" => {
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let arguments = item
                            .get("input")
                            .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                            .unwrap_or_default();
                        blocks.push(ContentBlock::ToolUse(ToolCall {
                            id,
                            name,
                            arguments,
                        }));
                    }
                    "tool_result" => {
                        if role == Role::User {
                            let tool_call_id = item
                                .get("tool_use_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let is_error = item
                                .get("is_error")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false);
                            let output = extract_tool_result_text(item);
                            blocks.push(ContentBlock::ToolResult(ToolResult {
                                tool_call_id,
                                success: !is_error,
                                output,
                            }));
                        }
                    }
                    _ => {}
                }
            }
            blocks
        }
        _ => Vec::new(),
    }
}

fn extract_tool_result_text(item: &serde_json::Value) -> String {
    match item.get("content") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => {
            arr.iter()
                .filter_map(|c| {
                    if c.get("type").and_then(|v| v.as_str()) == Some("text") {
                        c.get("text").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => String::new(),
    }
}

/// Split text into `Text` and `CodeBlock` content blocks by detecting fenced code blocks.
pub fn parse_text_with_code_blocks(text: &str) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut code_language: Option<String> = None;
    let mut code_content = String::new();

    for line in text.lines() {
        if !in_code_block && line.starts_with("```") {
            // Start of a code block
            if !current_text.is_empty() {
                blocks.push(ContentBlock::Text(current_text.trim_end().to_string()));
                current_text.clear();
            }
            let lang = line.trim_start_matches('`').trim();
            code_language = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            code_content.clear();
            in_code_block = true;
        } else if in_code_block && line.starts_with("```") {
            // End of a code block
            blocks.push(ContentBlock::CodeBlock {
                language: code_language.take(),
                code: code_content.trim_end().to_string(),
            });
            code_content.clear();
            in_code_block = false;
        } else if in_code_block {
            if !code_content.is_empty() {
                code_content.push('\n');
            }
            code_content.push_str(line);
        } else {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        }
    }

    // Handle unclosed code block
    if in_code_block && !code_content.is_empty() {
        blocks.push(ContentBlock::CodeBlock {
            language: code_language,
            code: code_content.trim_end().to_string(),
        });
    } else if !current_text.is_empty() {
        blocks.push(ContentBlock::Text(current_text.trim_end().to_string()));
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_with_no_code_blocks() {
        let blocks = parse_text_with_code_blocks("Hello world\nSecond line");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], ContentBlock::Text(t) if t == "Hello world\nSecond line"));
    }

    #[test]
    fn parse_text_with_single_code_block() {
        let input = "Before\n```rust\nfn main() {}\n```\nAfter";
        let blocks = parse_text_with_code_blocks(input);
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], ContentBlock::Text(t) if t == "Before"));
        assert!(
            matches!(&blocks[1], ContentBlock::CodeBlock { language, code } if language.as_deref() == Some("rust") && code == "fn main() {}")
        );
        assert!(matches!(&blocks[2], ContentBlock::Text(t) if t == "After"));
    }

    #[test]
    fn parse_text_with_no_language_code_block() {
        let input = "```\nsome code\n```";
        let blocks = parse_text_with_code_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert!(
            matches!(&blocks[0], ContentBlock::CodeBlock { language, code } if language.is_none() && code == "some code")
        );
    }

    #[test]
    fn decode_project_name_basic() {
        assert_eq!(
            decode_project_name("V--Projects-agent-history"),
            "V:/Projects-agent-history"
        );
    }
}
