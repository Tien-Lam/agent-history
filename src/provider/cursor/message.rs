use super::format::{
    millis_value_to_datetime, optional_string, optional_text, value_u8, BubbleData, ToolCallData,
    ToolFormerData,
};
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::parse_common::{epoch_timestamp_for_index, tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_message(
    bubble_id: &str,
    header_type: Option<u8>,
    value: &[u8],
    idx: usize,
) -> Option<Message> {
    let raw: BubbleData = serde_json::from_slice(value).ok()?;

    let bubble_type = raw
        .bubble_type
        .as_ref()
        .and_then(value_u8)
        .or(header_type)?;
    let role = match bubble_type {
        1 => Role::User,
        2 => Role::Assistant,
        _ => return None,
    };

    // Stable fallback when the bubble has no `createdAt`: anchor on the
    // Unix epoch + `idx` seconds. This is far in the past so it sorts
    // before any real session, but `idx` still preserves intra-session
    // ordering when the caller hands us the header position.
    let timestamp = raw
        .created_at
        .as_ref()
        .and_then(millis_value_to_datetime)
        .unwrap_or_else(|| epoch_timestamp_for_index(idx));

    let mut content: Vec<ContentBlock> = Vec::new();

    if let Some(text) = optional_text(raw.text.as_ref(), &["text", "content", "message"]) {
        content.extend(parse_text_with_code_blocks(&text));
    } else if let Some(text) = optional_text(
        raw.rich_text.as_ref(),
        &["richText", "text", "content", "message"],
    ) {
        content.extend(parse_text_with_code_blocks(&text));
    }

    for cb in &raw.code_blocks {
        let Some(body) = optional_text(cb.code.as_ref(), &["code", "content", "text"])
            .or_else(|| optional_text(cb.content.as_ref(), &["content", "code", "text"]))
        else {
            continue;
        };
        content.push(ContentBlock::CodeBlock {
            language: optional_string(cb.language.as_ref(), &["languageId", "language", "id"]),
            code: body,
        });
    }

    if let Some(tool) = &raw.tool_former {
        push_tool(tool, &mut content);
    }
    for tc in &raw.tool_calls {
        push_tool_v2(tc, &mut content);
    }

    Some(Message {
        id: MessageId(bubble_id.to_string()),
        role,
        timestamp,
        content,
        model: optional_string(raw.model.as_ref(), &["model", "id", "name"]),
        token_usage: None,
    })
}

fn push_tool(tool: &ToolFormerData, content: &mut Vec<ContentBlock>) {
    let id = optional_string(tool.tool_call_id.as_ref(), &["toolCallId", "id"])
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name =
        optional_string(tool.name.as_ref(), &["name", "toolName"]).unwrap_or_else(|| "tool".into());
    let arguments = stringify_json(&tool.params);

    content.push(tool_use_block(id.clone(), name, arguments));

    if !tool.result.is_null() {
        content.push(tool_result_block(
            id,
            is_success_status(tool.status.as_ref()),
            stringify_json(&tool.result),
        ));
    }
}

fn push_tool_v2(tc: &ToolCallData, content: &mut Vec<ContentBlock>) {
    let id = optional_string(tc.id.as_ref(), &["id", "toolCallId"])
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name =
        optional_string(tc.name.as_ref(), &["name", "toolName"]).unwrap_or_else(|| "tool".into());
    let arguments = stringify_json(&tc.arguments);

    content.push(tool_use_block(id.clone(), name, arguments));

    if !tc.result.is_null() {
        content.push(tool_result_block(
            id,
            is_success_status(tc.status.as_ref()),
            stringify_json(&tc.result),
        ));
    }
}

fn is_success_status(status: Option<&serde_json::Value>) -> bool {
    optional_string(status, &["status", "state"])
        .as_deref()
        .is_none_or(|s| !s.eq_ignore_ascii_case("error"))
}

fn stringify_json(v: &serde_json::Value) -> String {
    if v.is_null() {
        return String::new();
    }
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    serde_json::to_string(v).unwrap_or_default()
}
