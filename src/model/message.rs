use chrono::{DateTime, Utc};
use serde::Serialize;

use super::session::TokenUsage;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MessageId(pub String);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: MessageId,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
    pub token_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "You",
            Self::Assistant => "Assistant",
            Self::System => "System",
            Self::Tool => "Tool",
        }
    }

    /// Stable lowercase slug used for filtering and machine-readable contexts
    /// (e.g. `--role` CLI filter, search index storage). Distinct from
    /// [`Role::as_str`] which returns display-friendly labels.
    pub fn slug(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
        }
    }

    /// Inverse of [`Role::slug`]. Returns `None` for unknown slugs.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(String),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    ToolUse(ToolCall),
    ToolResult(ToolResult),
    Thinking(String),
    Error(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub success: bool,
    pub output: String,
}

#[cfg(test)]
mod role_tests {
    use super::Role;

    #[test]
    fn slug_round_trips_for_all_roles() {
        for role in [Role::User, Role::Assistant, Role::System, Role::Tool] {
            assert_eq!(Role::from_slug(role.slug()), Some(role));
        }
    }

    #[test]
    fn slug_is_lowercase_kebab_friendly() {
        assert_eq!(Role::User.slug(), "user");
        assert_eq!(Role::Assistant.slug(), "assistant");
        assert_eq!(Role::System.slug(), "system");
        assert_eq!(Role::Tool.slug(), "tool");
    }

    #[test]
    fn as_str_preserves_display_labels() {
        // `as_str` is a display-facing label, distinct from the machine slug.
        assert_eq!(Role::User.as_str(), "You");
        assert_eq!(Role::Assistant.as_str(), "Assistant");
    }

    #[test]
    fn from_slug_rejects_unknown_and_uppercase() {
        assert_eq!(Role::from_slug(""), None);
        assert_eq!(Role::from_slug("USER"), None);
        assert_eq!(Role::from_slug("robot"), None);
    }
}
