use serde_json::Value;

use super::super::{text_parts, RawContent, RawMessage, RawToolCall, Thought};
use crate::model::{ContentBlock, Role};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish};
use crate::provider::parse_common::{pretty_json_opt, tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(super) fn message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();

    let text = message_text(msg, role);
    if !text.is_empty() {
        content.extend(parse_text_with_code_blocks(&text));
    }

    append_thoughts(&mut content, msg.thoughts.as_deref());
    append_tool_calls(&mut content, msg.tool_calls.as_deref());

    content
}

fn message_text(msg: &RawMessage, role: Role) -> String {
    match &msg.content {
        RawContent::Text(s) => s.clone(),
        RawContent::Parts(parts) if role == Role::User => {
            let preferred = msg.display_content.as_deref().unwrap_or(parts);
            text_parts(preferred)
        }
        RawContent::Parts(parts) => text_parts(parts),
        RawContent::Json(value) => {
            string_or_object_field_or_pretty(value, &["text", "content", "message"])
        }
    }
}

fn append_thoughts(content: &mut Vec<ContentBlock>, thoughts: Option<&[Thought]>) {
    let Some(thoughts) = thoughts else {
        return;
    };

    for thought in thoughts {
        if let Some(desc) = stringish(
            thought.description.as_ref(),
            &["description", "text", "content"],
        )
        .filter(|desc| !desc.is_empty())
        {
            content.push(ContentBlock::Thinking(desc));
        }
    }
}

fn append_tool_calls(content: &mut Vec<ContentBlock>, tool_calls: Option<&[RawToolCall]>) {
    let Some(tool_calls) = tool_calls else {
        return;
    };

    for tc in tool_calls {
        let id = stringish(tc.id.as_ref(), &["id", "toolCallId"]).unwrap_or_default();
        content.push(tool_use_block(
            id.clone(),
            stringish(tc.name.as_ref(), &["name", "toolName"])
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(tc.args.as_ref()),
        ));

        append_tool_response(content, tc, id);
    }
}

fn append_tool_response(content: &mut Vec<ContentBlock>, tc: &RawToolCall, id: String) {
    let Some(output) = tc.response.as_ref().map(extract_tool_response_text) else {
        return;
    };

    if !output.is_empty() {
        content.push(tool_result_block(id, tc.error.is_none(), output));
    }
}

fn extract_tool_response_text(v: &Value) -> String {
    string_or_object_field_or_pretty(v, &["output", "result", "content", "text"])
}
