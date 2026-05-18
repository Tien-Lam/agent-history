use serde::Deserialize;

use crate::model::ContentBlock;
use crate::provider::json_text::{string_or_pretty, string_or_typed_text_array_or_pretty};
use crate::provider::parse_common::{tool_result_block, tool_use_block};
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
        "tool_use" => vec![tool_use_block(
            block.id.unwrap_or_default(),
            block.name.unwrap_or_default(),
            block
                .input
                .as_ref()
                .map(string_or_pretty)
                .unwrap_or_default(),
        )],
        "tool_result" => vec![tool_result_block(
            block.tool_use_id.unwrap_or_default(),
            true,
            block
                .content
                .as_ref()
                .map(|value| string_or_typed_text_array_or_pretty(value, "text", "text"))
                .unwrap_or_default(),
        )],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests;
