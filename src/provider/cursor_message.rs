use chrono::{TimeZone, Utc};

use super::cursor_format::{millis_to_datetime, BubbleData, ToolCallData, ToolFormerData};
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::parse_common::{tool_result_block, tool_use_block};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_message(
    bubble_id: &str,
    header_type: Option<u8>,
    value: &[u8],
    idx: usize,
) -> Option<Message> {
    let raw: BubbleData = serde_json::from_slice(value).ok()?;

    let role = match raw.bubble_type.or(header_type)? {
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
        .and_then(millis_to_datetime)
        .unwrap_or_else(|| {
            Utc.timestamp_opt(i64::try_from(idx).unwrap_or(0), 0)
                .single()
                .unwrap_or_else(Utc::now)
        });

    let mut content: Vec<ContentBlock> = Vec::new();

    if let Some(text) = raw.text.as_deref().filter(|s| !s.is_empty()) {
        content.extend(parse_text_with_code_blocks(text));
    } else if let Some(text) = raw.rich_text.as_deref().filter(|s| !s.is_empty()) {
        content.extend(parse_text_with_code_blocks(text));
    }

    for cb in &raw.code_blocks {
        let body = cb.code.as_deref().or(cb.content.as_deref()).unwrap_or("");
        if body.is_empty() {
            continue;
        }
        content.push(ContentBlock::CodeBlock {
            language: cb.language.clone(),
            code: body.to_string(),
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
        model: raw.model,
        token_usage: None,
    })
}

fn push_tool(tool: &ToolFormerData, content: &mut Vec<ContentBlock>) {
    let id = tool
        .tool_call_id
        .clone()
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name = tool.name.clone().unwrap_or_else(|| String::from("tool"));
    let arguments = stringify_json(&tool.params);

    content.push(tool_use_block(id.clone(), name, arguments));

    if !tool.result.is_null() {
        content.push(tool_result_block(
            id,
            tool.status
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("error")),
            stringify_json(&tool.result),
        ));
    }
}

fn push_tool_v2(tc: &ToolCallData, content: &mut Vec<ContentBlock>) {
    let id = tc.id.clone().unwrap_or_else(|| String::from("cursor-tool"));
    let name = tc.name.clone().unwrap_or_else(|| String::from("tool"));
    let arguments = stringify_json(&tc.arguments);

    content.push(tool_use_block(id.clone(), name, arguments));

    if !tc.result.is_null() {
        content.push(tool_result_block(
            id,
            tc.status
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("error")),
            stringify_json(&tc.result),
        ));
    }
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
