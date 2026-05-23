use serde_json::Value;

use super::super::record::RawMessage;
use crate::model::{ContentBlock, Role};
use crate::provider::json_text::{
    string_or_object_field_or_pretty, string_or_typed_text_array_or_pretty, stringish, value_bool,
};
use crate::provider::parse_common::{pretty_json_opt, tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(super) fn parse_message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let Some(content) = &msg.content else {
        return Vec::new();
    };

    match content {
        Value::String(s) => parse_text_with_code_blocks(s),
        Value::Array(arr) => {
            let mut blocks = Vec::new();
            for item in arr {
                let item_type = stringish(item.get("type"), &["type"]).unwrap_or_default();
                match item_type.as_str() {
                    "text" => {
                        if let Some(text) = item
                            .get("text")
                            .map(|v| string_or_object_field_or_pretty(v, &["text", "content"]))
                            .filter(|text| !text.is_empty())
                        {
                            blocks.extend(parse_text_with_code_blocks(&text));
                        }
                    }
                    "thinking" => {
                        if let Some(text) = item
                            .get("thinking")
                            .map(|v| string_or_object_field_or_pretty(v, &["thinking", "text"]))
                            .filter(|text| !text.is_empty())
                        {
                            blocks.push(ContentBlock::Thinking(text));
                        }
                    }
                    "tool_use" => {
                        let name = stringish(item.get("name"), &["name", "tool", "toolName"])
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| "unknown".to_string());
                        let id = stringish(item.get("id"), &["id"]).unwrap_or_default();
                        let arguments = pretty_json_opt(item.get("input"));
                        blocks.push(tool_use_block(id, name, arguments));
                    }
                    "tool_result" if role == Role::User => {
                        let tool_call_id =
                            stringish(item.get("tool_use_id"), &["tool_use_id", "toolUseId", "id"])
                                .unwrap_or_default();
                        let is_error =
                            value_bool(item.get("is_error"), &["is_error", "isError", "value"])
                                .unwrap_or(false);
                        let output = extract_tool_result_text(item);
                        blocks.push(tool_result_block(tool_call_id, !is_error, output));
                    }
                    _ => {}
                }
            }
            blocks
        }
        Value::Object(_) => {
            let text = string_or_object_field_or_pretty(content, &["text", "content", "message"]);
            if text.is_empty() {
                Vec::new()
            } else {
                parse_text_with_code_blocks(&text)
            }
        }
        _ => Vec::new(),
    }
}

fn extract_tool_result_text(item: &Value) -> String {
    item.get("content")
        .map(|content| match content {
            Value::Object(_) => {
                string_or_object_field_or_pretty(content, &["text", "content", "message", "output"])
            }
            _ => string_or_typed_text_array_or_pretty(content, "text", "text"),
        })
        .unwrap_or_default()
}
