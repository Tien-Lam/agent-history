use serde::Deserialize;
use serde_json::Value;

use crate::model::ContentBlock;
use crate::provider::json_text::{
    string_or_object_field, string_or_object_field_or_pretty, string_or_pretty,
    string_or_typed_text_array_or_pretty,
};
use crate::provider::parse_common::{tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Default)]
pub(crate) struct AnthropicContent(Value);

impl<'de> Deserialize<'de> for AnthropicContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Value::deserialize(deserializer).map(Self)
    }
}

pub(crate) fn content_to_blocks(content: AnthropicContent) -> Vec<ContentBlock> {
    match content.0 {
        Value::String(text) if !text.trim().is_empty() => parse_text_with_code_blocks(&text),
        Value::Array(blocks) => blocks.into_iter().flat_map(block_to_content).collect(),
        Value::Object(_) => text_blocks_from_string(&string_or_object_field_or_pretty(
            &content.0,
            &["text", "content", "message"],
        )),
        _ => vec![],
    }
}

fn block_to_content(block: Value) -> Vec<ContentBlock> {
    let Value::Object(map) = block else {
        return vec![];
    };
    let kind = map.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "text" => {
            let text = map
                .get("text")
                .map(|value| string_or_object_field_or_pretty(value, &["text", "content"]))
                .unwrap_or_default();
            text_blocks_from_string(&text)
        }
        "tool_use" => vec![tool_use_block(
            stringish_field(map.get("id"), &["id"]).unwrap_or_default(),
            stringish_field(map.get("name"), &["name", "tool", "toolName"])
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            map.get("input").map(string_or_pretty).unwrap_or_default(),
        )],
        "tool_result" => vec![tool_result_block(
            stringish_field(map.get("tool_use_id"), &["tool_use_id", "toolUseId"])
                .unwrap_or_default(),
            true,
            map.get("content")
                .map(|value| string_or_typed_text_array_or_pretty(value, "text", "text"))
                .unwrap_or_default(),
        )],
        _ => vec![],
    }
}

fn text_blocks_from_string(text: &str) -> Vec<ContentBlock> {
    if text.trim().is_empty() {
        vec![]
    } else {
        parse_text_with_code_blocks(text)
    }
}

fn stringish_field(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let value = value?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(_) => {
            let text = string_or_object_field(value, object_fields);
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
