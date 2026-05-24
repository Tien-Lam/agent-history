use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_u64};
use crate::provider::parse_common::{
    epoch_timestamp_for_index, timestamp_value_to_utc, token_usage_from_options,
    visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

mod raw;
mod tools;

use raw::RawEvent;
use tools::{
    push_nested_tool_requests, push_tool_execution_start, push_tool_result, push_top_level_tool_use,
};

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
            let fallback_idx = record.line_number.saturating_sub(1);
            let event = record.value;
            let event_type =
                stringish(event.event_type.as_ref(), &["type", "event"]).unwrap_or_default();
            let event_type_str = event_type.as_str();

            let role = match event_type_str {
                t if t.contains("user") => Role::User,
                t if t.contains("assistant.message") => Role::Assistant,
                "tool.execution_start" => {
                    push_tool_execution_start(&mut messages, &event, fallback_idx);
                    return;
                }
                "tool.execution_complete" | "tool.result" => {
                    push_tool_result(&mut messages, &event, fallback_idx);
                    return;
                }
                t if t.contains("tool") => Role::Tool,
                _ => {
                    skipped_records += 1;
                    tracing::trace!(event_type = event_type_str, "skipping non-message event");
                    return;
                }
            };

            let timestamp = event_timestamp(&event, fallback_idx);
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
        parse_stats: ProviderParseStats::from_counts(
            stats.line_count,
            stats.parse_errors,
            skipped_records,
            empty_content,
        ),
    })
}

fn event_timestamp(event: &RawEvent, fallback_idx: usize) -> DateTime<Utc> {
    copilot_timestamp(event.timestamp.as_ref())
        .unwrap_or_else(|| epoch_timestamp_for_index(fallback_idx))
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
