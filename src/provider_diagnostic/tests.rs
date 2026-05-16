use chrono::Utc;

use super::*;
use crate::model::{MessageId, Role, ToolCall, ToolResult};

fn msg(content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId("m".to_string()),
        role: Role::Assistant,
        timestamp: Utc::now(),
        content,
        model: None,
        token_usage: None,
    }
}

fn tu(id: &str, name: &str, args: &str) -> ContentBlock {
    ContentBlock::ToolUse(ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: args.to_string(),
    })
}

fn tr(id: &str, ok: bool) -> ContentBlock {
    ContentBlock::ToolResult(ToolResult {
        tool_call_id: id.to_string(),
        success: ok,
        output: String::new(),
    })
}

#[test]
fn analyze_counts_block_kinds() {
    let messages = vec![msg(vec![
        ContentBlock::Text("hi".into()),
        ContentBlock::CodeBlock {
            language: None,
            code: "x".into(),
        },
        ContentBlock::Thinking("...".into()),
        ContentBlock::Error("boom".into()),
    ])];
    let (blocks, _) = analyze_messages(&messages);
    assert_eq!(blocks.text, 1);
    assert_eq!(blocks.code_block, 1);
    assert_eq!(blocks.thinking, 1);
    assert_eq!(blocks.error, 1);
    assert_eq!(blocks.total, 4);
}

#[test]
fn analyze_pairs_tool_calls_with_results_by_id() {
    let messages = vec![msg(vec![
        tu("call-1", "Bash", "{}"),
        tu("call-2", "Read", "{}"),
        tr("call-1", true),
        tr("call-3", false),
    ])];
    let (_, f) = analyze_messages(&messages);
    assert_eq!(f.tool_calls, 2);
    assert_eq!(f.tool_results, 2);
    assert_eq!(f.paired, 1, "call-1 has a matching result");
    assert_eq!(f.unpaired_calls, 1, "call-2 has no result");
    assert_eq!(f.orphan_results, 1, "call-3 result has no call");
    assert_eq!(f.success_results, 1);
    assert_eq!(f.failure_results, 1);
}

#[test]
fn analyze_flags_empty_name_and_id_and_invalid_args() {
    let messages = vec![msg(vec![tu("", "", "not-json"), tu("ok", "Read", "")])];
    let (_, f) = analyze_messages(&messages);
    assert_eq!(f.empty_names, 1);
    assert_eq!(f.empty_call_ids, 1);
    assert_eq!(f.invalid_json_args, 1);
}

#[test]
fn analyze_handles_empty_input() {
    let (blocks, fidelity) = analyze_messages(&[]);
    assert_eq!(blocks, BlockCounts::default());
    assert_eq!(fidelity, ToolCallFidelity::default());
}
