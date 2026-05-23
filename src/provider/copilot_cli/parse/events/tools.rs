use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_bool};
use crate::provider::parse_common::{pretty_json_opt, tool_result_block, tool_use_block};

use super::event_timestamp;
use super::raw::{RawEvent, RawEventData};

pub(super) fn push_tool_execution_start(messages: &mut Vec<Message>, event: &RawEvent) {
    let Some(data) = event.data.as_ref() else {
        return;
    };
    messages.push(tool_message(
        event,
        vec![tool_use_block(
            stringish(data.tool_call_id.as_ref(), &["toolCallId", "id"]).unwrap_or_default(),
            data.tool_name
                .as_ref()
                .and_then(|value| stringish(Some(value), &["name", "toolName", "tool"]))
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(data.arguments.as_ref()),
        )],
    ));
}

pub(super) fn push_tool_result(messages: &mut Vec<Message>, event: &RawEvent) {
    let Some(data) = event.data.as_ref() else {
        return;
    };
    let output = data
        .result
        .as_ref()
        .map(extract_result_text)
        .unwrap_or_default();
    if output.is_empty() {
        return;
    }
    messages.push(tool_message(
        event,
        vec![tool_result_block(
            stringish(data.tool_call_id.as_ref(), &["toolCallId", "id"]).unwrap_or_default(),
            value_bool(data.success.as_ref(), &["success", "ok", "value"]).unwrap_or(true),
            output,
        )],
    ));
}

pub(super) fn push_top_level_tool_use(content: &mut Vec<ContentBlock>, event: &RawEvent) {
    if let Some(tool_name) = &event.tool_name {
        content.push(tool_use_block(
            stringish(event.tool_call_id.as_ref(), &["toolCallId", "id"]).unwrap_or_default(),
            stringish(Some(tool_name), &["name", "toolName", "tool"])
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(event.tool_args.as_ref()),
        ));
    }
}

pub(super) fn push_nested_tool_requests(
    content: &mut Vec<ContentBlock>,
    data: Option<&RawEventData>,
) {
    let Some(tool_requests) = data.and_then(|d| d.tool_requests.as_ref()) else {
        return;
    };
    for tr in tool_requests {
        content.push(tool_use_block(
            stringish(tr.tool_call_id.as_ref(), &["toolCallId", "id"]).unwrap_or_default(),
            tr.name
                .as_ref()
                .and_then(|value| stringish(Some(value), &["name", "toolName", "tool"]))
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(tr.arguments.as_ref()),
        ));
    }
}

fn tool_message(event: &RawEvent, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(stringish(event.id.as_ref(), &["id"]).unwrap_or_default()),
        role: Role::Tool,
        timestamp: event_timestamp(event),
        content,
        model: None,
        token_usage: None,
    }
}

fn extract_result_text(v: &serde_json::Value) -> String {
    string_or_object_field_or_pretty(
        v,
        &["detailedContent", "content", "output", "result", "text"],
    )
}
