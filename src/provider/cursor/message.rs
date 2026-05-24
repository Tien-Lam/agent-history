use super::format::{millis_value_to_datetime, BubbleData, ToolCallData, ToolFormerData};
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{non_empty_string_or_object_field_or_pretty, stringish, value_u8};
use crate::provider::parse_common::{epoch_timestamp_for_index, tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) enum BuildMessageResult {
    Message(Message),
    ParseError,
    SkippedRecord,
}

pub(crate) fn build_message_result(
    bubble_id: &str,
    header_type: Option<u8>,
    value: &[u8],
    idx: usize,
) -> BuildMessageResult {
    let Ok(raw) = serde_json::from_slice::<BubbleData>(value) else {
        return BuildMessageResult::ParseError;
    };

    let Some(bubble_type) = value_u8(raw.bubble_type.as_ref(), &["type", "value"]).or(header_type)
    else {
        return BuildMessageResult::SkippedRecord;
    };
    let role = match bubble_type {
        1 => Role::User,
        2 => Role::Assistant,
        _ => return BuildMessageResult::SkippedRecord,
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

    if let Some(text) = non_empty_string_or_object_field_or_pretty(
        raw.text.as_ref(),
        &["text", "content", "message"],
    ) {
        content.extend(parse_text_with_code_blocks(&text));
    } else if let Some(text) = non_empty_string_or_object_field_or_pretty(
        raw.rich_text.as_ref(),
        &["richText", "text", "content", "message"],
    ) {
        content.extend(parse_text_with_code_blocks(&text));
    }

    for cb in &raw.code_blocks {
        let Some(body) = non_empty_string_or_object_field_or_pretty(
            cb.code.as_ref(),
            &["code", "content", "text"],
        )
        .or_else(|| {
            non_empty_string_or_object_field_or_pretty(
                cb.content.as_ref(),
                &["content", "code", "text"],
            )
        }) else {
            continue;
        };
        content.push(ContentBlock::CodeBlock {
            language: stringish(cb.language.as_ref(), &["languageId", "language", "id"]),
            code: body,
        });
    }

    if let Some(tool) = &raw.tool_former {
        push_tool(tool, &mut content);
    }
    for tc in &raw.tool_calls {
        push_tool_v2(tc, &mut content);
    }

    BuildMessageResult::Message(Message {
        id: MessageId(bubble_id.to_string()),
        role,
        timestamp,
        content,
        model: stringish(raw.model.as_ref(), &["model", "id", "name"]),
        token_usage: None,
    })
}

fn push_tool(tool: &ToolFormerData, content: &mut Vec<ContentBlock>) {
    let id = stringish(tool.tool_call_id.as_ref(), &["toolCallId", "id"])
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name =
        stringish(tool.name.as_ref(), &["name", "toolName"]).unwrap_or_else(|| "tool".into());
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
    let id = stringish(tc.id.as_ref(), &["id", "toolCallId"])
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name = stringish(tc.name.as_ref(), &["name", "toolName"]).unwrap_or_else(|| "tool".into());
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
    stringish(status, &["status", "state"])
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
    v.to_string()
}
