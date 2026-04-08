use chrono::{DateTime, Utc};

use super::session::TokenUsage;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: MessageId,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
    pub token_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub success: bool,
    pub output: String,
}
