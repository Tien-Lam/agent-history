use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::parse_common::{
    parse_utc, parse_utc_or_now, pretty_json_opt, tool_result_block, tool_use_block,
    visit_jsonl_records,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn build_session_from_rollout(path: &Path) -> Option<Session> {
    let mut first_timestamp: Option<DateTime<Utc>> = None;
    let mut last_timestamp: Option<DateTime<Utc>> = None;
    let mut message_count: usize = 0;
    let mut first_user_message: Option<String> = None;

    visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let entry = record.value;
            if let Some(ts) = &entry.timestamp {
                if let Some(dt) = parse_utc(ts) {
                    if first_timestamp.is_none() {
                        first_timestamp = Some(dt);
                    }
                    last_timestamp = Some(dt);
                }
            }

            match entry.entry_type.as_deref() {
                Some("user" | "assistant") => {
                    message_count += 1;
                    if entry.entry_type.as_deref() == Some("user") && first_user_message.is_none() {
                        first_user_message = entry.content.map(|c| c.chars().take(80).collect());
                    }
                }
                Some("event_msg") => {
                    // Newer Codex format
                    if let Some(ref payload) = entry.payload {
                        if let Some("user_message" | "agent_message") =
                            payload.entry_type.as_deref()
                        {
                            message_count += 1;
                            if payload.entry_type.as_deref() == Some("user_message")
                                && first_user_message.is_none()
                            {
                                first_user_message = payload
                                    .message
                                    .as_ref()
                                    .map(|m| m.chars().take(80).collect());
                            }
                        }
                    }
                }
                _ => {}
            }
        },
        |_| {},
    )
    .ok()?;

    if message_count == 0 {
        return None;
    }

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::CodexCli,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at: first_timestamp?,
        ended_at: last_timestamp,
        summary: first_user_message,
        model: None,
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    })
}

pub(crate) fn parse_rollout_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Codex CLI messages");
    let mut messages = Vec::new();
    let mut skipped_types: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let line_number = record.line_number;
            let entry = record.value;
            let entry_type = entry.entry_type.as_deref().unwrap_or("");
            let role = match entry_type {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool_use" => Role::Tool,
                "error" => {
                    if let Some(error_msg) = entry.error.as_deref() {
                        messages.push(error_message(
                            entry_timestamp(&entry),
                            error_msg.to_string(),
                        ));
                    }
                    return;
                }
                "event_msg" => {
                    push_event_msg(&mut messages, &entry);
                    return;
                }
                "response_item" => {
                    push_response_item(&mut messages, &entry);
                    return;
                }
                _ => {
                    skipped_types += 1;
                    tracing::trace!(entry_type, "skipping non-message entry");
                    return;
                }
            };

            let timestamp = entry_timestamp(&entry);
            let content = legacy_content(&entry, role);

            if content.is_empty() {
                empty_content += 1;
                tracing::trace!(
                    line_num = line_number,
                    entry_type,
                    "skipping entry with empty content"
                );
                return;
            }

            messages.push(message(role, timestamp, content));
        },
        |error| {
            tracing::warn!(line_num = error.line_number, error = %error.error, "failed to parse JSONL line");
        },
    )?;

    tracing::info!(
        path = %path.display(),
        lines = stats.line_count,
        parse_errors = stats.parse_errors,
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Codex CLI message loading complete"
    );

    assign_fallback_message_ids(&mut messages);

    Ok(messages)
}

fn entry_timestamp(entry: &RawEntry) -> DateTime<Utc> {
    parse_utc_or_now(entry.timestamp.as_deref())
}

fn message(role: Role, timestamp: DateTime<Utc>, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(String::new()),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    }
}

fn assign_fallback_message_ids(messages: &mut [Message]) {
    for (idx, message) in messages.iter_mut().enumerate() {
        if message.id.0.is_empty() {
            message.id = MessageId(format!("codex-turn-{}", idx + 1));
        }
    }
}

fn error_message(timestamp: DateTime<Utc>, error_msg: String) -> Message {
    message(
        Role::System,
        timestamp,
        vec![ContentBlock::Error(error_msg)],
    )
}

fn push_text_message(
    messages: &mut Vec<Message>,
    role: Role,
    timestamp: DateTime<Utc>,
    text: &str,
) {
    if !text.is_empty() {
        messages.push(message(role, timestamp, parse_text_with_code_blocks(text)));
    }
}

fn push_event_msg(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = payload.entry_type.as_deref().unwrap_or("");
    let timestamp = entry_timestamp(entry);
    match payload_type {
        "user_message" => {
            if let Some(msg_text) = payload.message.as_deref() {
                push_text_message(messages, Role::User, timestamp, msg_text);
            }
        }
        "agent_message" => {
            if let Some(msg_text) = payload.message.as_deref() {
                push_text_message(messages, Role::Assistant, timestamp, msg_text);
            }
        }
        _ => tracing::trace!(payload_type, "skipping event_msg"),
    }
}

fn push_response_item(messages: &mut Vec<Message>, entry: &RawEntry) {
    let Some(payload) = entry.payload.as_ref() else {
        return;
    };
    let payload_type = payload.entry_type.as_deref().unwrap_or("");
    let timestamp = entry_timestamp(entry);
    match payload_type {
        "function_call" => messages.push(message(
            Role::Tool,
            timestamp,
            vec![tool_use_block(
                payload.call_id.clone().unwrap_or_default(),
                payload
                    .name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                payload.arguments.clone().unwrap_or_default(),
            )],
        )),
        "function_call_output" => {
            let output = payload.output.clone().unwrap_or_default();
            if !output.is_empty() {
                messages.push(message(
                    Role::Tool,
                    timestamp,
                    vec![tool_result_block(
                        payload.call_id.clone().unwrap_or_default(),
                        true,
                        output,
                    )],
                ));
            }
        }
        _ => tracing::trace!(payload_type, "skipping response_item"),
    }
}

fn legacy_content(entry: &RawEntry, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    let Some(text) = entry.content.as_ref().filter(|text| !text.is_empty()) else {
        return content;
    };
    if role == Role::Tool {
        content.push(tool_use_block(
            String::new(),
            text.clone(),
            pretty_json_opt(entry.tool_calls.as_ref()),
        ));
    } else {
        content.extend(parse_text_with_code_blocks(text));
    }
    content
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    content: Option<String>,
    timestamp: Option<String>,
    tool_calls: Option<serde_json::Value>,
    error: Option<String>,
    /// Newer Codex format wraps messages in a payload object
    payload: Option<RawPayload>,
}

#[derive(Deserialize)]
struct RawPayload {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    /// `event_msg`: user/agent message text
    message: Option<String>,
    /// `response_item` `function_call`: tool name
    name: Option<String>,
    /// `response_item` `function_call`: call ID
    call_id: Option<String>,
    /// `response_item` `function_call`: arguments as JSON string
    arguments: Option<String>,
    /// `response_item` `function_call_output`: output text
    output: Option<String>,
}
