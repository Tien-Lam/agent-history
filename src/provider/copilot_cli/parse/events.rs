use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{
    string_or_object_field_or_pretty, stringish, value_bool, value_u64,
};
use crate::provider::parse_common::{
    pretty_json_opt, timestamp_value_to_utc, token_usage_from_options, tool_result_block,
    tool_use_block, visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

pub(crate) fn parse_events_jsonl(path: &Path) -> Result<Vec<Message>, ProviderError> {
    Ok(parse_events_jsonl_with_stats(path)?.messages)
}

pub(crate) fn parse_events_jsonl_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Copilot CLI messages");
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawEvent, _, _>(
        path,
        |record| {
            let event = record.value;
            let event_type =
                stringish(event.event_type.as_ref(), &["type", "event"]).unwrap_or_default();
            let event_type_str = event_type.as_str();

            let role = match event_type_str {
                t if t.contains("user") => Role::User,
                t if t.contains("assistant.message") => Role::Assistant,
                "tool.execution_start" => {
                    push_tool_execution_start(&mut messages, &event);
                    return;
                }
                "tool.execution_complete" | "tool.result" => {
                    push_tool_result(&mut messages, &event);
                    return;
                }
                t if t.contains("tool") => Role::Tool,
                _ => {
                    skipped_records += 1;
                    tracing::trace!(event_type = event_type_str, "skipping non-message event");
                    return;
                }
            };

            let timestamp = event_timestamp(&event);
            let content = event_content(&event);

            if content.is_empty() {
                empty_content += 1;
                tracing::debug!(
                    event_type = event_type_str,
                    has_data = event.data.is_some(),
                    data_has_content = event.data.as_ref().is_some_and(|d| d.content.is_some()),
                    "skipping event with empty content"
                );
                return;
            }

            messages.push(event_message(&event, role, timestamp, content));
        },
        |error| {
            tracing::warn!(line_num = error.line_number, error = %error.error, "failed to parse JSONL line");
        },
    )?;

    tracing::info!(
        path = %path.display(),
        lines = stats.line_count,
        parse_errors = stats.parse_errors,
        skipped_records,
        empty_content,
        messages = messages.len(),
        "Copilot CLI message loading complete"
    );

    Ok(ProviderMessageLoad {
        messages,
        parse_stats: ProviderParseStats {
            records_seen: stats.line_count,
            parse_errors: stats.parse_errors,
            skipped_records,
            empty_content,
        },
    })
}

fn event_timestamp(event: &RawEvent) -> DateTime<Utc> {
    copilot_timestamp(event.timestamp.as_ref()).unwrap_or_else(Utc::now)
}

fn copilot_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(value, &["timestamp", "time", "createdAt", "value"])
}

fn event_message(
    event: &RawEvent,
    role: Role,
    timestamp: DateTime<Utc>,
    content: Vec<ContentBlock>,
) -> Message {
    Message {
        id: MessageId(stringish(event.id.as_ref(), &["id"]).unwrap_or_default()),
        role,
        timestamp,
        content,
        model: event
            .model
            .as_ref()
            .and_then(|value| stringish(Some(value), &["model", "id", "name"])),
        token_usage: event.usage.as_ref().map(|u| {
            token_usage_from_options(
                value_u64(u.input_tokens.as_ref()),
                value_u64(u.output_tokens.as_ref()),
                None,
                None,
            )
        }),
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

fn push_tool_execution_start(messages: &mut Vec<Message>, event: &RawEvent) {
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

fn push_tool_result(messages: &mut Vec<Message>, event: &RawEvent) {
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

fn event_content(event: &RawEvent) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    let text = event.content.as_ref().map(event_text).or_else(|| {
        event
            .data
            .as_ref()
            .and_then(|d| d.content.as_ref().map(event_text))
    });
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        content.extend(parse_text_with_code_blocks(&text));
    }
    push_top_level_tool_use(&mut content, event);
    push_nested_tool_requests(&mut content, event.data.as_ref());
    content
}

fn event_text(value: &Value) -> String {
    string_or_object_field_or_pretty(value, &["content", "text", "message"])
}

fn push_top_level_tool_use(content: &mut Vec<ContentBlock>, event: &RawEvent) {
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

fn push_nested_tool_requests(content: &mut Vec<ContentBlock>, data: Option<&RawEventData>) {
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

#[derive(Deserialize)]
struct RawEvent {
    id: Option<Value>,
    #[serde(rename = "type")]
    event_type: Option<Value>,
    timestamp: Option<Value>,
    content: Option<Value>,
    model: Option<Value>,
    #[serde(rename = "toolName")]
    tool_name: Option<Value>,
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<Value>,
    #[serde(rename = "toolArgs")]
    tool_args: Option<Value>,
    usage: Option<RawUsage>,
    data: Option<RawEventData>,
}

#[derive(Deserialize)]
struct RawEventData {
    content: Option<Value>,
    #[serde(rename = "toolRequests")]
    tool_requests: Option<Vec<RawToolRequest>>,
    #[serde(rename = "toolName")]
    tool_name: Option<Value>,
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<Value>,
    arguments: Option<Value>,
    success: Option<Value>,
    result: Option<Value>,
}

fn extract_result_text(v: &Value) -> String {
    string_or_object_field_or_pretty(
        v,
        &["detailedContent", "content", "output", "result", "text"],
    )
}

#[derive(Deserialize)]
struct RawToolRequest {
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<Value>,
    name: Option<Value>,
    arguments: Option<Value>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<Value>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<Value>,
}
