mod types;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};

use crate::error::{Error, Result};
use crate::model::{Message, MessageRole, Session, SessionId, TokenUsage, ToolCall};
use crate::paths;
use crate::provider::Provider;

use types::{ContentBlock, RawContent, RawEntry};

pub struct ClaudeProvider {
    projects_dir: PathBuf,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            projects_dir: paths::claude_projects_dir(),
        }
    }

    #[cfg(test)]
    fn with_projects_dir(dir: PathBuf) -> Self {
        Self { projects_dir: dir }
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn is_available(&self) -> bool {
        self.projects_dir.is_dir()
    }

    fn discover_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();

        let project_dirs = fs::read_dir(&self.projects_dir).map_err(|e| Error::ReadSession {
            path: self.projects_dir.clone(),
            source: e,
        })?;

        for project_entry in project_dirs {
            let project_entry = project_entry?;
            let project_dir = project_entry.path();
            if !project_dir.is_dir() {
                continue;
            }

            let dir_name = project_entry.file_name();
            let project_path = paths::decode_project_path(&dir_name.to_string_lossy());

            let Ok(jsonl_files) = fs::read_dir(&project_dir) else {
                continue;
            };

            for file_entry in jsonl_files {
                let Ok(file_entry) = file_entry else {
                    continue;
                };
                let file_path = file_entry.path();

                if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }

                let session_id = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                match discover_session_metadata(&file_path, &session_id, &project_path) {
                    Ok(session) => sessions.push(session),
                    Err(e) => {
                        eprintln!("warning: skipping {}: {e}", file_path.display());
                    }
                }
            }
        }

        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>> {
        parse_messages(&session.source_path)
    }
}

/// Read the first N lines of a JSONL file to extract session metadata without full parsing.
fn discover_session_metadata(path: &Path, session_id: &str, project_path: &str) -> Result<Session> {
    let file = fs::File::open(path).map_err(|e| Error::ReadSession {
        path: path.to_path_buf(),
        source: e,
    })?;
    let metadata = file.metadata().map_err(|e| Error::ReadSession {
        path: path.to_path_buf(),
        source: e,
    })?;
    let updated_at = metadata
        .modified()
        .ok()
        .and_then(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
            Utc.timestamp_opt(duration.as_secs().cast_signed(), 0)
                .single()
        })
        .unwrap_or_else(Utc::now);

    let reader = BufReader::new(file);
    let mut summary = String::new();
    let mut model = None;
    let mut started_at = None;
    let mut message_count: usize = 0;

    for line in reader.lines().take(100) {
        let Ok(line) = line else { continue };
        if line.is_empty() {
            continue;
        }

        let entry: RawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry {
            RawEntry::User { message, timestamp } => {
                message_count += 1;
                if started_at.is_none() {
                    started_at = parse_timestamp(&timestamp);
                }
                if summary.is_empty()
                    && let RawContent::Text(text) = &message.content
                {
                    summary = truncate_summary(text);
                }
            }
            RawEntry::Assistant { message, timestamp } => {
                message_count += 1;
                if started_at.is_none() {
                    started_at = parse_timestamp(&timestamp);
                }
                if model.is_none() {
                    model = message.model;
                }
            }
            RawEntry::QueueOperation { .. } | RawEntry::Attachment { .. } => {}
        }
    }

    Ok(Session {
        id: SessionId::new(session_id),
        provider: "claude".to_string(),
        project_path: project_path.to_string(),
        started_at: started_at.unwrap_or(updated_at),
        updated_at,
        summary,
        model,
        message_count,
        source_path: path.to_path_buf(),
    })
}

/// Parse all messages from a Claude Code JSONL file.
fn parse_messages(path: &Path) -> Result<Vec<Message>> {
    let file = fs::File::open(path).map_err(|e| Error::ReadSession {
        path: path.to_path_buf(),
        source: e,
    })?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.is_empty() {
            continue;
        }

        let entry: RawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                eprintln!(
                    "warning: parse error at {}:{}: {e}",
                    path.display(),
                    line_num + 1
                );
                continue;
            }
        };

        match entry {
            RawEntry::User { message, timestamp } => {
                let ts = parse_timestamp(&timestamp);
                let msg = convert_message(MessageRole::User, message, ts);
                if !msg.content.is_empty() || !msg.tool_calls.is_empty() {
                    messages.push(msg);
                }
            }
            RawEntry::Assistant { message, timestamp } => {
                let ts = parse_timestamp(&timestamp);
                let msg = convert_message(MessageRole::Assistant, message, ts);
                messages.push(msg);
            }
            RawEntry::QueueOperation { .. } | RawEntry::Attachment { .. } => {}
        }
    }

    Ok(messages)
}

/// Convert a raw Claude message into our common `Message` type.
fn convert_message(
    role: MessageRole,
    raw: types::RawMessage,
    timestamp: Option<DateTime<Utc>>,
) -> Message {
    let model = raw.model;
    let token_usage = raw.usage.map(|u| TokenUsage {
        input_tokens: u.input_tokens,
        output_tokens: u.output_tokens,
        cache_read_tokens: u.cache_read_input_tokens,
        cache_creation_tokens: u.cache_creation_input_tokens,
    });

    let mut content_parts = Vec::new();
    let mut thinking = None;
    let mut tool_calls = Vec::new();

    match raw.content {
        RawContent::Text(text) => {
            content_parts.push(text);
        }
        RawContent::Blocks(blocks) => {
            for block in blocks {
                match block {
                    ContentBlock::Text { text } => {
                        content_parts.push(text);
                    }
                    ContentBlock::Thinking { thinking: t } => {
                        thinking = Some(t);
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        let preview = truncate_json_preview(&input);
                        tool_calls.push(ToolCall {
                            name,
                            input_preview: preview,
                        });
                    }
                    ContentBlock::ToolResult { .. } => {}
                }
            }
        }
    }

    Message {
        role,
        content: content_parts.join("\n"),
        timestamp,
        model,
        tool_calls,
        token_usage,
        thinking,
    }
}

fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn truncate_summary(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or(text);
    if first_line.len() > 200 {
        format!("{}…", &first_line[..199])
    } else {
        first_line.to_string()
    }
}

fn truncate_json_preview(value: &serde_json::Value) -> String {
    let s = value.to_string();
    if s.len() > 100 {
        format!("{}…", &s[..99])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_jsonl(dir: &std::path::Path, session_id: &str, lines: &[&str]) -> PathBuf {
        let project_dir = dir.join("-home-user-test-project");
        fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join(format!("{session_id}.jsonl"));
        let mut file = fs::File::create(&file_path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file_path
    }

    #[test]
    fn discover_sessions_finds_jsonl_files() {
        let tmp = tempdir();
        let lines = &[
            r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-04-08T11:06:26.991Z","sessionId":"test-123"}"#,
            r#"{"type":"user","message":{"role":"user","content":"Hello, help me write code"},"timestamp":"2026-04-08T11:06:27.024Z","uuid":"u1","sessionId":"test-123"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-6","content":[{"type":"text","text":"Sure, I can help."}],"usage":{"input_tokens":10,"output_tokens":5}},"timestamp":"2026-04-08T11:06:30.638Z"}"#,
        ];
        create_test_jsonl(&tmp, "test-session-id", lines);

        let provider = ClaudeProvider::with_projects_dir(tmp.clone());
        let sessions = provider.discover_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.as_ref(), "test-session-id");
        assert_eq!(sessions[0].provider, "claude");
        assert_eq!(sessions[0].project_path, "/home/user/test/project");
        assert_eq!(sessions[0].summary, "Hello, help me write code");
        assert_eq!(sessions[0].model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(sessions[0].message_count, 2);
    }

    #[test]
    fn load_messages_parses_all_types() {
        let tmp = tempdir();
        let lines = &[
            r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-04-08T11:06:26.991Z","sessionId":"s1"}"#,
            r#"{"type":"user","message":{"role":"user","content":"What is 2+2?"},"timestamp":"2026-04-08T11:00:00.000Z","uuid":"u1","sessionId":"s1"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-6","content":[{"type":"thinking","thinking":"Simple math","signature":"sig"},{"type":"text","text":"The answer is 4."}],"usage":{"input_tokens":10,"output_tokens":8}},"timestamp":"2026-04-08T11:00:01.000Z"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","id":"t1","input":{"command":"echo 4"}}]},"timestamp":"2026-04-08T11:00:02.000Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"4\n"}]},"timestamp":"2026-04-08T11:00:03.000Z"}"#,
        ];
        let path = create_test_jsonl(&tmp, "msg-test", lines);

        let session = Session {
            id: SessionId::new("msg-test"),
            provider: "claude".to_string(),
            project_path: "/test".to_string(),
            started_at: Utc::now(),
            updated_at: Utc::now(),
            summary: String::new(),
            model: None,
            message_count: 0,
            source_path: path,
        };

        let provider = ClaudeProvider::with_projects_dir(tmp);
        let messages = provider.load_messages(&session).unwrap();

        assert!(messages.len() >= 3);

        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "What is 2+2?");

        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert_eq!(messages[1].content, "The answer is 4.");
        assert_eq!(messages[1].thinking.as_deref(), Some("Simple math"));
        assert_eq!(messages[1].model.as_deref(), Some("claude-opus-4-6"));
        assert!(messages[1].token_usage.is_some());

        assert_eq!(messages[2].role, MessageRole::Assistant);
        assert_eq!(messages[2].tool_calls.len(), 1);
        assert_eq!(messages[2].tool_calls[0].name, "Bash");
    }

    use std::sync::atomic::{AtomicU32, Ordering};

    fn tempdir() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "agent-history-test-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
