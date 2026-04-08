use serde::Deserialize;
use serde_json::Value;

/// Top-level JSONL entry. Each line in a Claude Code session file is one of these.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RawEntry {
    QueueOperation {
        #[allow(dead_code)]
        operation: String,
    },
    User {
        message: RawMessage,
        timestamp: String,
    },
    Assistant {
        message: RawMessage,
        timestamp: String,
    },
    Attachment {
        #[allow(dead_code)]
        attachment: Value,
    },
}

#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[allow(dead_code)]
    pub role: String,
    pub content: RawContent,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<RawUsage>,
}

/// Message content: either a plain string (user text) or an array of content blocks.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Individual content block within a message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        name: String,
        #[allow(dead_code)]
        id: String,
        input: Value,
    },
    ToolResult {
        #[allow(dead_code)]
        tool_use_id: String,
        #[serde(default)]
        #[allow(dead_code)]
        is_error: bool,
        #[serde(default)]
        #[allow(dead_code)]
        content: Value,
    },
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct RawUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_text_entry() {
        let json = r#"{"type":"user","message":{"role":"user","content":"Hello world"},"timestamp":"2026-04-08T11:06:27.024Z","uuid":"abc","sessionId":"def"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        match entry {
            RawEntry::User { message, .. } => match message.content {
                RawContent::Text(t) => assert_eq!(t, "Hello world"),
                _ => panic!("expected text content"),
            },
            _ => panic!("expected User entry"),
        }
    }

    #[test]
    fn parse_assistant_with_tool_use() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-6","content":[{"type":"text","text":"Let me check."},{"type":"tool_use","name":"Bash","id":"toolu_123","input":{"command":"ls"}}],"usage":{"input_tokens":100,"output_tokens":50}},"timestamp":"2026-04-08T11:06:30.638Z"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        match entry {
            RawEntry::Assistant { message, .. } => {
                assert_eq!(message.model.as_deref(), Some("claude-opus-4-6"));
                assert!(message.usage.is_some());
                match message.content {
                    RawContent::Blocks(blocks) => {
                        assert_eq!(blocks.len(), 2);
                        assert!(
                            matches!(&blocks[0], ContentBlock::Text { text } if text == "Let me check.")
                        );
                        assert!(
                            matches!(&blocks[1], ContentBlock::ToolUse { name, .. } if name == "Bash")
                        );
                    }
                    _ => panic!("expected blocks content"),
                }
            }
            _ => panic!("expected Assistant entry"),
        }
    }

    #[test]
    fn parse_thinking_block() {
        let json = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me think about this...","signature":"sig123"}]},"timestamp":"2026-04-08T11:06:30.638Z"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        match entry {
            RawEntry::Assistant { message, .. } => match message.content {
                RawContent::Blocks(blocks) => {
                    assert!(
                        matches!(&blocks[0], ContentBlock::Thinking { thinking } if thinking == "Let me think about this...")
                    );
                }
                _ => panic!("expected blocks"),
            },
            _ => panic!("expected Assistant"),
        }
    }

    #[test]
    fn parse_queue_operation() {
        let json = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-04-08T11:06:26.991Z","sessionId":"abc"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, RawEntry::QueueOperation { .. }));
    }

    #[test]
    fn parse_attachment() {
        let json = r#"{"type":"attachment","attachment":{"type":"deferred_tools_delta"},"uuid":"abc","timestamp":"2026-04-08T11:06:27.022Z"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, RawEntry::Attachment { .. }));
    }

    #[test]
    fn parse_tool_result_block() {
        let json = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_123","is_error":false,"content":"file contents here"}]},"timestamp":"2026-04-08T11:06:31.255Z"}"#;
        let entry: RawEntry = serde_json::from_str(json).unwrap();
        match entry {
            RawEntry::User { message, .. } => match message.content {
                RawContent::Blocks(blocks) => {
                    assert!(
                        matches!(&blocks[0], ContentBlock::ToolResult { is_error, .. } if !is_error)
                    );
                }
                _ => panic!("expected blocks"),
            },
            _ => panic!("expected User"),
        }
    }
}
