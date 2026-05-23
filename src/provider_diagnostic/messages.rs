use crate::model::{ContentBlock, Message};

use super::{BlockCounts, ToolCallFidelity};

/// Aggregate `[ContentBlock]` and tool-call statistics for a flat slice of
/// messages. Pairing is computed within this slice only, so callers wanting
/// per-session pairing should call this per session.
#[must_use]
pub fn analyze_messages(messages: &[Message]) -> (BlockCounts, ToolCallFidelity) {
    let mut blocks = BlockCounts::default();
    let mut fidelity = ToolCallFidelity::default();

    let mut call_ids: Vec<String> = Vec::new();
    let mut result_ids: Vec<String> = Vec::new();

    for msg in messages {
        for block in &msg.content {
            blocks.total += 1;
            match block {
                ContentBlock::Text(_) => blocks.text += 1,
                ContentBlock::CodeBlock { .. } => blocks.code_block += 1,
                ContentBlock::Thinking(_) => blocks.thinking += 1,
                ContentBlock::Error(_) => blocks.error += 1,
                ContentBlock::ToolUse(tc) => {
                    blocks.tool_use += 1;
                    fidelity.tool_calls += 1;
                    if tc.name.is_empty() {
                        fidelity.empty_names += 1;
                    }
                    if tc.id.is_empty() {
                        fidelity.empty_call_ids += 1;
                    } else {
                        call_ids.push(tc.id.clone());
                    }
                    if !tc.arguments.is_empty()
                        && serde_json::from_str::<serde_json::Value>(&tc.arguments).is_err()
                    {
                        fidelity.invalid_json_args += 1;
                    }
                }
                ContentBlock::ToolResult(tr) => {
                    blocks.tool_result += 1;
                    fidelity.tool_results += 1;
                    if tr.success {
                        fidelity.success_results += 1;
                    } else {
                        fidelity.failure_results += 1;
                    }
                    if tr.tool_call_id.is_empty() {
                        fidelity.empty_result_ids += 1;
                    } else {
                        result_ids.push(tr.tool_call_id.clone());
                    }
                }
            }
        }
    }

    let result_set: std::collections::HashSet<&str> =
        result_ids.iter().map(String::as_str).collect();
    let call_set: std::collections::HashSet<&str> = call_ids.iter().map(String::as_str).collect();

    for id in &call_ids {
        if result_set.contains(id.as_str()) {
            fidelity.paired += 1;
        } else {
            fidelity.unpaired_calls += 1;
        }
    }
    for id in &result_ids {
        if !call_set.contains(id.as_str()) {
            fidelity.orphan_results += 1;
        }
    }

    (blocks, fidelity)
}
