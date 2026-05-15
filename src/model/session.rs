use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::citation::{CitationRef, SessionRef};
use super::provider::Provider;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Session {
    pub id: SessionId,
    pub provider: Provider,
    pub project_path: Option<PathBuf>,
    pub project_name: Option<String>,
    pub git_branch: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub token_usage: Option<TokenUsage>,
    pub message_count: usize,
    pub source_path: PathBuf,
}

impl Session {
    pub fn session_ref(&self) -> SessionRef {
        SessionRef {
            provider: self.provider,
            session_id: self.id.clone(),
        }
    }

    pub fn citation_ref(&self, turn: u32) -> Option<CitationRef> {
        self.session_ref().turn(turn)
    }

    /// Internal key for caches and indexes. Provider session ids are not
    /// globally unique, so include provider and source path as well.
    pub fn identity_key(&self) -> String {
        format!(
            "{}\x1f{}\x1f{}",
            self.provider.slug(),
            self.id.0,
            self.source_path.display()
        )
    }

    /// Internal key for message-level caches. Some providers emit empty or
    /// repeated message ids, so include the 0-based turn index.
    pub fn message_key(&self, turn_index: usize, message_id: &str) -> String {
        format!(
            "{}\x1f{}\x1f{}",
            self.identity_key(),
            turn_index,
            message_id
        )
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}
