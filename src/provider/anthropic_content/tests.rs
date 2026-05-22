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

#[test]
fn content_to_blocks_accepts_object_content() {
    let blocks = content_to_blocks(
        serde_json::from_value(json!({
            "text": "object text"
        }))
        .unwrap(),
    );

    assert!(matches!(&blocks[0], ContentBlock::Text(text) if text == "object text"));
}

#[test]
fn content_to_blocks_skips_malformed_blocks_without_dropping_neighbors() {
    let blocks = content_to_blocks(
        serde_json::from_value(json!([
            { "text": "missing type" },
            { "type": "text", "text": { "content": "nested text" } },
            { "type": 12, "text": "bad type" },
            { "type": "text", "text": "plain text" }
        ]))
        .unwrap(),
    );

    let rendered = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(rendered, vec!["nested text", "plain text"]);
}

#[test]
fn content_to_blocks_uses_unknown_tool_name_when_missing() {
    let blocks = content_to_blocks(
        serde_json::from_value(json!([
            { "type": "tool_use", "id": 7, "input": { "path": "src/lib.rs" } }
        ]))
        .unwrap(),
    );

    assert!(
        matches!(&blocks[0], ContentBlock::ToolUse(tool) if tool.id == "7" && tool.name == "unknown")
    );
}
