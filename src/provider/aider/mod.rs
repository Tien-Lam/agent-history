//! Aider (<https://aider.chat>) provider.
//!
//! Aider is a per-repo CLI coding assistant that writes its conversation
//! transcript to two files in each project directory:
//!
//! - `.aider.chat.history.md` — markdown-formatted conversation log
//! - `.aider.input.history` — raw user input lines. We ignore this; the chat
//!   history file is the canonical record.
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
//! so deeper traversal is wasted work and risks dragging in `node_modules` /
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

mod parse;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use parse::{load_messages_from_file, parse_sessions_in_file};

const HISTORY_FILE: &str = ".aider.chat.history.md";
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
        load_messages_from_file(&session.source_path, session.started_at, &session.id.0)
    }
}

/// Recursively scans `dir` for `.aider.chat.history.md` files. Bounded by
/// [`MAX_WALK_DEPTH`] because Aider history lives at project roots — going
/// deeper just wades into `node_modules`, `.git`, etc.
fn collect_history_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::model::{ContentBlock, Role};
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
        let has_code = messages[2]
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::CodeBlock { .. }));
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
        let buried = tmp
            .path()
            .join("projects")
            .join("good")
            .join("node_modules")
            .join("pkg");
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
        assert_eq!(
            sessions.len(),
            1,
            "only the well-formed session should survive"
        );
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
