use tantivy::schema::{Field, Value};
use tantivy::TantivyDocument;

use crate::model::{ContentBlock, Message};

pub(super) fn extract_content(message: &Message) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                parts.push(t.as_str());
            }
            ContentBlock::CodeBlock { code, .. } => parts.push(code.as_str()),
            ContentBlock::ToolUse(tc) => parts.push(tc.arguments.as_str()),
            ContentBlock::ToolResult(_) => {}
        }
    }
    parts.join("\n")
}

pub(super) fn message_has_tool_call(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse(_)))
}

pub(super) fn extract_tool_output(message: &Message) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for block in &message.content {
        if let ContentBlock::ToolResult(tr) = block {
            parts.push(tr.output.as_str());
        }
    }
    parts.join("\n")
}

pub(super) fn field_text(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(super) fn field_i64(doc: &TantivyDocument, field: Field) -> Option<i64> {
    doc.get_first(field).and_then(|v| v.as_i64())
}
