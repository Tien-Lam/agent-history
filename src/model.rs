use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Newtype for session identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SessionId(String);

impl SessionId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Check if the given prefix matches the start of this session ID.
    #[must_use]
    pub fn matches_prefix(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }

    /// Return the first 8 characters as a short display form.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..self.0.len().min(8)]
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: SessionId,
    pub provider: String,
    pub project_path: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// First user text message, truncated.
    pub summary: String,
    pub model: Option<String>,
    pub message_count: usize,
    #[serde(skip)]
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub token_usage: Option<TokenUsage>,
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => f.write_str("user"),
            Self::Assistant => f.write_str("assistant"),
            Self::System => f.write_str("system"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub name: String,
    /// Truncated preview of the tool input JSON.
    pub input_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_matches_prefix() {
        let id = SessionId::new("8106598e-ef44-4c81-a363-67f9dcc1a4d7");
        assert!(id.matches_prefix("8106598e"));
        assert!(id.matches_prefix("81"));
        assert!(!id.matches_prefix("abcd"));
    }

    #[test]
    fn session_id_short() {
        let id = SessionId::new("8106598e-ef44-4c81-a363-67f9dcc1a4d7");
        assert_eq!(id.short(), "8106598e");
    }

    #[test]
    fn session_id_short_handles_short_ids() {
        let id = SessionId::new("abc");
        assert_eq!(id.short(), "abc");
    }
}
