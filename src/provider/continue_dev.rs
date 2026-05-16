//! Continue.dev VS Code/JetBrains extension provider.
//!
//! Continue (<https://continue.dev>) stores each chat session as a JSONL file
//! under `~/.continue/sessions/`. On disk:
//!
//! - All platforms: `~/.continue/sessions/<uuid>.jsonl`
//!
//! An optional `~/.continue/sessions/index.json` contains a session index with
//! titles and timestamps, used when present to enrich session metadata.
//!
//! Each JSONL file has one JSON object per line:
//!
//! ```json
//! {"role": "user", "content": "message text"}
//! {"role": "assistant", "content": "response text"}
//! ```
//!
//! Content can be a plain string or an array of Anthropic-style content blocks
//! (`{"type": "text", "text": "..."}` etc.). Both forms are handled.
//!
//! The session ID is the JSONL filename stem (UUID or similar). Timestamps are
//! derived from the index file when available, falling back to file mtime.

use std::cmp::Reverse;
use std::path::PathBuf;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use crate::provider::continue_dev_parse::{build_session_from_file, load_index, parse_jsonl};

const SESSIONS_SUBDIR: &str = "sessions";

pub struct ContinueDevProvider {
    dirs: Vec<PathBuf>,
}

impl ContinueDevProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.join(SESSIONS_SUBDIR).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(continue_home) = std::env::var("CONTINUE_HOME") {
        result.push(PathBuf::from(continue_home));
        return result;
    }

    if let Some(home) = super::home_dir() {
        result.push(home.join(".continue"));
    }

    result
}

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ContinueDevProvider {
    fn provider(&self) -> Provider {
        Provider::ContinueDev
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let sessions_dir = base.join(SESSIONS_SUBDIR);
            if !sessions_dir.is_dir() {
                continue;
            }

            // Load index for enriched metadata (optional)
            let index = load_index(&sessions_dir);

            let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let Some(ext) = path.extension() else {
                    continue;
                };
                if ext != "jsonl" {
                    continue;
                }
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let session_id = stem.to_string();
                sessions.push(build_session_from_file(path, session_id, index.as_deref()));
            }
        }

        sessions.sort_by_key(|s| Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_jsonl(&session.source_path, &session.started_at).map_err(|reason| {
            ProviderError::Parse {
                path: session.source_path.clone(),
                reason,
            }
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, Role};
    use crate::provider::continue_dev_parse::INDEX_FILE;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn sessions_dir(root: &Path) -> PathBuf {
        root.join(SESSIONS_SUBDIR)
    }

    fn write_session(root: &Path, id: &str, lines: &str) -> PathBuf {
        let sd = sessions_dir(root);
        fs::create_dir_all(&sd).unwrap();
        let path = sd.join(format!("{id}.jsonl"));
        fs::write(&path, lines).unwrap();
        path
    }

    fn provider_for(tmp: &TempDir) -> ContinueDevProvider {
        ContinueDevProvider::new(vec![tmp.path().to_path_buf()])
    }

    #[test]
    fn discover_returns_empty_when_sessions_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let p = provider_for(&tmp);
        assert!(p.discover_sessions().unwrap().is_empty());
    }

    #[test]
    fn discovers_jsonl_session_files() {
        let tmp = TempDir::new().unwrap();
        write_session(tmp.path(), "abc-uuid", r#"{"role":"user","content":"hi"}"#);
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "abc-uuid");
    }

    #[test]
    fn ignores_non_jsonl_files() {
        let tmp = TempDir::new().unwrap();
        let sd = sessions_dir(tmp.path());
        fs::create_dir_all(&sd).unwrap();
        fs::write(sd.join("index.json"), "[]").unwrap();
        fs::write(sd.join("session.txt"), "nope").unwrap();
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn uses_index_title_as_summary() {
        let tmp = TempDir::new().unwrap();
        let sd = sessions_dir(tmp.path());
        fs::create_dir_all(&sd).unwrap();
        fs::write(
            sd.join(INDEX_FILE),
            r#"[{"sessionId":"my-uuid","title":"My Session","dateCreated":"2026-01-01T00:00:00Z"}]"#,
        )
        .unwrap();
        write_session(tmp.path(), "my-uuid", r#"{"role":"user","content":"hi"}"#);
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions[0].summary.as_deref(), Some("My Session"));
        // 2026-01-01T00:00:00Z
        assert_eq!(sessions[0].started_at.timestamp(), 1_767_225_600);
    }

    #[test]
    fn parses_string_content() {
        let tmp = TempDir::new().unwrap();
        write_session(
            tmp.path(),
            "uuid1",
            "{\"role\":\"user\",\"content\":\"hello\"}\n{\"role\":\"assistant\",\"content\":\"hi\"}",
        );
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[test]
    fn parses_block_content() {
        let tmp = TempDir::new().unwrap();
        write_session(
            tmp.path(),
            "uuid1",
            r#"{"role":"user","content":[{"type":"text","text":"block text"}]}"#,
        );
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(&msgs[0].content[0], ContentBlock::Text(_)));
    }

    #[test]
    fn skips_unknown_roles() {
        let tmp = TempDir::new().unwrap();
        write_session(
            tmp.path(),
            "uuid1",
            "{\"role\":\"system\",\"content\":\"sys\"}\n{\"role\":\"user\",\"content\":\"hi\"}",
        );
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        // system is now included (Role::System)
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn load_errors_on_corrupt_jsonl() {
        let tmp = TempDir::new().unwrap();
        write_session(tmp.path(), "uuid1", "not json");
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        assert!(p.load_messages(&sessions[0]).is_err());
    }
}
