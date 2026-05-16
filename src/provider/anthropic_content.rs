use serde::Deserialize;

use crate::model::{ContentBlock, ToolCall, ToolResult};
use crate::provider::json_text::{string_or_pretty, string_or_typed_text_array_or_pretty};
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Deserialize, Default)]
#[serde(untagged)]
pub(crate) enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicBlock>),
    #[default]
    Empty,
}

#[derive(Deserialize)]
pub(crate) struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

pub(crate) fn content_to_blocks(content: AnthropicContent) -> Vec<ContentBlock> {
    match content {
        AnthropicContent::Text(text) if !text.trim().is_empty() => {
            parse_text_with_code_blocks(&text)
        }
        AnthropicContent::Blocks(blocks) => blocks.into_iter().flat_map(block_to_content).collect(),
        _ => vec![],
    }
}

fn block_to_content(block: AnthropicBlock) -> Vec<ContentBlock> {
    match block.kind.as_str() {
        "text" => {
            let text = block.text.unwrap_or_default();
            if text.trim().is_empty() {
                vec![]
            } else {
                parse_text_with_code_blocks(&text)
            }
        }
        "tool_use" => vec![ContentBlock::ToolUse(ToolCall {
            id: block.id.unwrap_or_default(),
            name: block.name.unwrap_or_default(),
            arguments: block
                .input
                .as_ref()
                .map(string_or_pretty)
                .unwrap_or_default(),
        })],
        "tool_result" => vec![ContentBlock::ToolResult(ToolResult {
            tool_call_id: block.tool_use_id.unwrap_or_default(),
            success: true,
            output: block
                .content
                .as_ref()
                .map(|value| string_or_typed_text_array_or_pretty(value, "text", "text"))
                .unwrap_or_default(),
        })],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn content_to_blocks_parses_plain_text() {
        let blocks = content_to_blocks(serde_json::from_value(json!("hello")).unwrap());
        assert!(matches!(&blocks[0], ContentBlock::Text(text) if text == "hello"));
    }

    #[test]
    fn content_to_blocks_parses_tool_use_arguments() {
        let blocks = content_to_blocks(
            serde_json::from_value(json!([
                { "type": "tool_use", "id": "call-1", "name": "read_file", "input": { "path": "src/main.rs" } }
            ]))
            .unwrap(),
        );
        assert!(
            matches!(&blocks[0], ContentBlock::ToolUse(tool) if tool.id == "call-1" && tool.arguments.contains("\"path\""))
        );
    }

    #[test]
    fn content_to_blocks_joins_tool_result_text_parts() {
        let blocks = content_to_blocks(
            serde_json::from_value(json!([
                {
                    "type": "tool_result",
                    "tool_use_id": "call-1",
                    "content": [
                        { "type": "text", "text": "first" },
                        { "type": "image", "text": "ignored" },
                        { "type": "text", "text": "second" }
                    ]
                }
            ]))
            .unwrap(),
        );
        assert!(
            matches!(&blocks[0], ContentBlock::ToolResult(result) if result.tool_call_id == "call-1" && result.output == "first\nsecond")
        );
    }
}
