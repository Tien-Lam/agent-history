use std::path::Path;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::text_blocks::parse_text_with_code_blocks;

const SESSION_HEADER: &str = "# aider chat started at ";
const USER_MARKER: &str = "####";

/// One contiguous chat block from a history file: the timestamp recovered
/// from the `# aider chat started at` header plus the body lines that follow.
struct SessionBlock {
    started_at: DateTime<Utc>,
    body: String,
}

/// Splits a full history file into one [`SessionBlock`] per
/// `# aider chat started at` header. Anything before the first header is
/// dropped, and headers with unparseable timestamps drop their section.
fn split_sessions(content: &str) -> Vec<SessionBlock> {
    let mut out: Vec<SessionBlock> = Vec::new();
    let mut current: Option<SessionBlock> = None;

    for line in content.lines() {
        if let Some(ts_str) = line.strip_prefix(SESSION_HEADER) {
            if let Some(block) = current.take() {
                out.push(block);
            }
            if let Some(started_at) = parse_session_timestamp(ts_str.trim()) {
                current = Some(SessionBlock {
                    started_at,
                    body: String::new(),
                });
            }
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.body.push_str(line);
            block.body.push('\n');
        }
    }
    if let Some(block) = current {
        out.push(block);
    }
    out
}

fn parse_session_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // Aider writes `YYYY-MM-DD HH:MM:SS` in local time without a tz suffix.
    // We treat it as UTC — the alternative (chrono::Local) is non-portable
    // and would make session ordering jitter across timezones. Sessions are
    // always rendered relative to one another so this is fine in practice.
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Utc.from_utc_datetime(&naive).into()
}

mod session;

pub(crate) use session::parse_sessions_in_file;

pub(crate) fn load_messages_from_file(
    path: &Path,
    target_started_at: DateTime<Utc>,
    session_id: &str,
) -> Result<Vec<Message>, ProviderError> {
    let bytes = std::fs::read_to_string(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    for block in split_sessions(&bytes) {
        if block.started_at == target_started_at {
            return Ok(parse_messages(&block, session_id));
        }
    }
    Ok(Vec::new())
}

struct Pending {
    role: Option<Role>,
    lines: Vec<String>,
}

impl Pending {
    fn new() -> Self {
        Self {
            role: None,
            lines: Vec::new(),
        }
    }

    fn open(&mut self, role: Role) {
        self.role = Some(role);
        self.lines.clear();
    }

    fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }
}

fn parse_messages(block: &SessionBlock, session_id: &str) -> Vec<Message> {
    let mut messages: Vec<Message> = Vec::new();
    let mut pending = Pending::new();
    let mut in_code_fence = false;
    let mut idx: i64 = 0;

    for line in block.body.lines() {
        // Code-fence guard: a `####` or `>` line inside a fenced block is
        // literal markdown, not a role marker.
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
            // A fence can only appear inside an assistant reply (user input
            // and tool output don't fence). If no role is open yet, it's the
            // assistant; otherwise keep the active role.
            if pending.role.is_none() {
                pending.open(Role::Assistant);
            }
            pending.push(line);
            continue;
        }

        if in_code_fence {
            pending.push(line);
            continue;
        }

        // Role markers
        if let Some(rest) = line.strip_prefix(USER_MARKER) {
            if pending.role != Some(Role::User) {
                flush_pending(
                    &mut pending,
                    &mut messages,
                    &mut idx,
                    block.started_at,
                    session_id,
                );
                pending.open(Role::User);
            }
            let stripped = rest.strip_prefix(' ').unwrap_or(rest);
            pending.push(stripped);
            continue;
        }
        if line.starts_with('>') {
            // Match `> rest` and bare `>`; preserve trailing content if any.
            let rest = line
                .strip_prefix("> ")
                .unwrap_or_else(|| line.strip_prefix('>').unwrap_or(""));
            if pending.role != Some(Role::Tool) {
                flush_pending(
                    &mut pending,
                    &mut messages,
                    &mut idx,
                    block.started_at,
                    session_id,
                );
                pending.open(Role::Tool);
            }
            pending.push(rest);
            continue;
        }

        // Blank line: keep within the active block (preserves paragraph
        // breaks); without an active role it's leading whitespace.
        if line.trim().is_empty() {
            if pending.role.is_some() {
                pending.push(line);
            }
            continue;
        }

        // Anything else is assistant prose. If we were mid-tool, the tool
        // block ended at the previous line.
        if pending.role != Some(Role::Assistant) {
            flush_pending(
                &mut pending,
                &mut messages,
                &mut idx,
                block.started_at,
                session_id,
            );
            pending.open(Role::Assistant);
        }
        pending.push(line);
    }
    flush_pending(
        &mut pending,
        &mut messages,
        &mut idx,
        block.started_at,
        session_id,
    );
    messages
}

fn flush_pending(
    pending: &mut Pending,
    messages: &mut Vec<Message>,
    idx: &mut i64,
    started_at: DateTime<Utc>,
    session_id: &str,
) {
    let Some(role) = pending.role.take() else {
        pending.lines.clear();
        return;
    };
    let body = pending.lines.join("\n");
    pending.lines.clear();
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    let content = match role {
        Role::Tool => vec![ContentBlock::Text(trimmed.to_string())],
        _ => parse_text_with_code_blocks(trimmed),
    };
    let timestamp = started_at + chrono::Duration::milliseconds(*idx);
    messages.push(Message {
        id: MessageId(format!("{session_id}#{idx}")),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    });
    *idx += 1;
}
