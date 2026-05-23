use crate::model::{ContentBlock, Message};

pub(super) fn collect_text(message: &Message) -> String {
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}
