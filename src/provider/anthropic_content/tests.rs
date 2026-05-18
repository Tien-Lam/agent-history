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
