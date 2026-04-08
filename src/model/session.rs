use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::provider::Provider;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}
