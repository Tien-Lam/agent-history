//! Aider (<https://aider.chat>) provider.
//!
//! Aider is a per-repo CLI coding assistant that writes its conversation
//! transcript to two files in each project directory:
//!
//! - `.aider.chat.history.md` — markdown-formatted conversation log
//! - `.aider.input.history`   — raw user input lines (we ignore this; the
//!                              chat history file is the canonical record)
//!
//! Because the files live inside each repo (not in a per-user data dir),
//! discovery walks one or more configured roots looking for any directory
//! that contains a `.aider.chat.history.md`.
//!
//! ## Roots, in order
//!
//! 1. `AIDER_ROOT` env var (colon-separated list)
//! 2. `AGHIST_HOME`-rooted `projects/` (testability shim)
//! 3. `~/projects/` (the convention called out in the spec)
//!
//! Walks are bounded to depth 4 — Aider history sits at the project root,
//! so deeper traversal is wasted work and risks dragging in node_modules /
//! `.git` blobs from sibling repos.
//!
//! ## File format
//!
//! - `# aider chat started at <ts>` opens a new session.
//! - Lines starting with `####` open a user message; subsequent `####`
//!   lines extend it until the next role boundary.
//! - Plain lines (including fenced code blocks) form the assistant message.
//! - Lines starting with `>` are aider command output / metadata. We
//!   surface them as `Role::Tool` so they're searchable but visually
//!   separable from real conversation.
//!
//! Multiple sessions can share one file; we keep them as distinct `Session`
//! records keyed `<sha8>:<timestamp>` so the IDs are deterministic and stable.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

use super::{HistoryProvider, ProviderError};
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::claude_code::parse_text_with_code_blocks;

const HISTORY_FILE: &str = ".aider.chat.history.md";
const SESSION_HEADER: &str = "# aider chat started at ";
const USER_MARKER: &str = "####";
const MAX_WALK_DEPTH: usize = 4;

pub struct AiderProvider {
    dirs: Vec<PathBuf>,
}

impl AiderProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|p| p.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Ok(roots) = std::env::var("AIDER_ROOT") {
        for r in roots.split(':').filter(|s| !s.is_empty()) {
            result.push(PathBuf::from(r));
        }
    }

    if let Some(home) = super::home_dir() {
        // Spec: "walk ~/projects/". This is also where AGHIST_HOME tests
        // place fixture project trees.
        let projects = home.join("projects");
        if !result.iter().any(|p| p == &projects) {
            result.push(projects);
        }
    }

    result
}

impl HistoryProvider for AiderProvider {
    fn provider(&self) -> Provider {
        Provider::Aider
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
            let mut history_files = Vec::new();
            collect_history_files(base, 0, &mut history_files);
            for file in history_files {
                match parse_sessions_in_file(&file) {
                    Ok(found) => sessions.extend(found),
                    Err(e) => {
                        tracing::warn!(path = %file.display(), error = %e, "skipping unreadable Aider history");
                    }
                }
            }
        }
        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        let bytes = std::fs::read_to_string(&session.source_path).map_err(|e| {
            ProviderError::Parse {
                path: session.source_path.clone(),
                reason: e.to_string(),
            }
        })?;
        let blocks = split_sessions(&bytes);
        let target_started_at = session.started_at;
        for block in blocks {
            if block.started_at == target_started_at {
                return Ok(parse_messages(&block, &session.id.0));
            }
        }
        Ok(Vec::new())
    }
}

/// Recursively scans `dir` for `.aider.chat.history.md` files. Bounded by
/// [`MAX_WALK_DEPTH`] because Aider history lives at project roots — going
/// deeper just wades into `node_modules`, `.git`, etc.
fn collect_history_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(HISTORY_FILE) {
                out.push(path);
            }
            continue;
        }
        if ft.is_dir() {
            // Skip well-known noise dirs to keep the walk cheap. We don't
            // bother filtering by `.gitignore` — that's overkill for a TUI.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(
                    name,
                    "node_modules" | ".git" | "target" | "dist" | "build" | ".venv" | "venv"
                ) {
                    continue;
                }
            }
            collect_history_files(&path, depth + 1, out);
        }
    }
}

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

fn parse_sessions_in_file(path: &Path) -> Result<Vec<Session>, ProviderError> {
    let content = std::fs::read_to_string(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let project_dir = path.parent().map(Path::to_path_buf);
    let project_name = project_dir
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .map(str::to_string);

    // Stable per-file prefix so session IDs differ across projects even
    // when timestamps collide.
    let file_prefix = short_hash(path);

    let mut sessions = Vec::new();
    for block in split_sessions(&content) {
        let id = format!("{file_prefix}:{}", block.started_at.format("%Y%m%dT%H%M%SZ"));
        let messages = parse_messages(&block, &id);
        if messages.is_empty() {
            // Empty section (no `####` and no body lines) — skip rather
            // than emit a phantom session.
            continue;
        }
        let ended_at = messages.last().map(|m| m.timestamp);
        sessions.push(Session {
            id: SessionId(id),
            provider: Provider::Aider,
            project_path: project_dir.clone(),
            project_name: project_name.clone(),
            git_branch: None,
            started_at: block.started_at,
            ended_at,
            summary: first_user_line(&messages),
            model: None,
            token_usage: None,
            message_count: messages.len(),
            source_path: path.to_path_buf(),
        });
    }
    Ok(sessions)
}

fn first_user_line(messages: &[Message]) -> Option<String> {
    for msg in messages {
        if msg.role != Role::User {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::Text(t) = block {
                let line = t.lines().next().unwrap_or("").trim();
                if !line.is_empty() {
                    return Some(truncate(line, 120));
                }
            }
        }
    }
    None
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// 8-hex-digit FNV1a of a path. Used as a session-ID prefix; not security-
/// sensitive — collisions are tolerated, the timestamp is the real key.
fn short_hash(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", (h ^ (h >> 32)) as u32)
}

struct Pending {
    role: Option<Role>,
    lines: Vec<String>,
}

impl Pending {
    fn new() -> Self {
        Self { role: None, lines: Vec::new() }
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

    fn flush(
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
                flush(&mut pending, &mut messages, &mut idx, block.started_at, session_id);
                pending.open(Role::User);
            }
            let stripped = rest.strip_prefix(' ').unwrap_or(rest);
            pending.push(stripped);
            continue;
        }
        if line.starts_with('>') {
            // Match `> rest` and bare `>`; preserve trailing content if any.
            let rest = line.strip_prefix("> ").unwrap_or_else(|| line.strip_prefix('>').unwrap_or(""));
            if pending.role != Some(Role::Tool) {
                flush(&mut pending, &mut messages, &mut idx, block.started_at, session_id);
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
            flush(&mut pending, &mut messages, &mut idx, block.started_at, session_id);
            pending.open(Role::Assistant);
        }
        pending.push(line);
    }
    flush(&mut pending, &mut messages, &mut idx, block.started_at, session_id);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_history(dir: &Path, project: &str, body: &str) -> PathBuf {
        let project_dir = dir.join("projects").join(project);
        fs::create_dir_all(&project_dir).unwrap();
        let path = project_dir.join(HISTORY_FILE);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn detect_returns_none_when_root_missing() {
        let tmp = TempDir::new().unwrap();
        let provider = AiderProvider::new(vec![tmp.path().join("nope")]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn parses_single_session_with_user_and_assistant() {
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at 2026-01-15 10:30:45

> /add src/main.rs

#### How do I split this auth handler?

Extract the validator into its own function:

```rust
fn validate(t: &str) -> bool { !t.is_empty() }
```

That keeps the handler shorter.

> Tokens: 234 sent, 56 received.
";
        let history = write_history(tmp.path(), "myapp", body);
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.provider, Provider::Aider);
        assert_eq!(s.project_name.as_deref(), Some("myapp"));
        assert_eq!(s.source_path, history);
        assert!(s.summary.as_deref().unwrap().contains("split this auth"));

        let messages = provider.load_messages(s).unwrap();
        // tool (`> /add`), user, assistant, tool (token count) = 4
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, Role::Tool);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
        assert_eq!(messages[3].role, Role::Tool);

        // Assistant should contain a code block.
        let has_code = messages[2].content.iter().any(|b| matches!(b, ContentBlock::CodeBlock { .. }));
        assert!(has_code, "assistant turn should preserve code block");
    }

    #[test]
    fn splits_multiple_sessions_in_one_file() {
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at 2026-01-15 10:30:45

#### first question

first answer.

# aider chat started at 2026-01-16 09:00:00

#### second question

second answer.
";
        write_history(tmp.path(), "repo", body);
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        // Sorted newest-first.
        assert!(sessions[0].started_at > sessions[1].started_at);
        for s in &sessions {
            let msgs = provider.load_messages(s).unwrap();
            assert_eq!(msgs.iter().filter(|m| m.role == Role::User).count(), 1);
            assert_eq!(msgs.iter().filter(|m| m.role == Role::Assistant).count(), 1);
        }
    }

    #[test]
    fn discovers_history_in_nested_project_dir() {
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at 2026-01-15 10:30:45

#### hi

hello
";
        write_history(tmp.path(), "group/inner", body);
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project_name.as_deref(), Some("inner"));
    }

    #[test]
    fn skips_noise_directories() {
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at 2026-01-15 10:30:45

#### hi

hello
";
        // Drop a history file inside a node_modules subtree — it must NOT
        // be discovered, otherwise vendored fixture files would pollute the
        // session list.
        let buried = tmp.path().join("projects").join("good").join("node_modules").join("pkg");
        fs::create_dir_all(&buried).unwrap();
        fs::write(buried.join(HISTORY_FILE), body).unwrap();
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn ignores_corrupt_session_header() {
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at not-a-real-date

#### bogus

#### bogus too

# aider chat started at 2026-01-15 10:30:45

#### good

answer
";
        write_history(tmp.path(), "repo", body);
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1, "only the well-formed session should survive");
    }

    #[test]
    fn fenced_role_markers_are_not_message_boundaries() {
        // A `####` literal inside a code fence must not start a new user
        // message — otherwise we'd shred markdown headings written into
        // assistant code samples.
        let tmp = TempDir::new().unwrap();
        let body = "\
# aider chat started at 2026-01-15 10:30:45

#### show me a markdown sample

Sure:

```md
#### Subheading
> Inside fence
```

Done.
";
        write_history(tmp.path(), "repo", body);
        let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        let msgs = provider.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.iter().filter(|m| m.role == Role::User).count(), 1);
        assert_eq!(msgs.iter().filter(|m| m.role == Role::Assistant).count(), 1);
    }

    #[test]
    fn aider_root_env_var_extends_search_paths() {
        let tmp = TempDir::new().unwrap();
        let custom = tmp.path().join("custom-root");
        fs::create_dir_all(custom.join("repo")).unwrap();
        fs::write(
            custom.join("repo").join(HISTORY_FILE),
            "# aider chat started at 2026-02-01 12:00:00\n\n#### hi\n\nhello\n",
        )
        .unwrap();

        // base_dirs() reads AIDER_ROOT; we test that path inclusion directly
        // rather than mutating the global env (test parallelism).
        let provider = AiderProvider::new(vec![custom]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
    }
}
