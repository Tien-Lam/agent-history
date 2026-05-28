use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use self::content::message_content;
use crate::fs_read;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{stringish, value_u64};
use crate::provider::parse_common::{
    epoch_timestamp_for_index, token_usage_from_options, MAX_PROVIDER_SESSION_FILE_BYTES,
};
use crate::provider::{ProviderError, ProviderMessageLoad, ProviderParseStats};

use super::{gemini_timestamp, raw_role, RawMessage};

mod content;

pub(crate) fn load_messages_from_path_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Gemini CLI messages");
    let data = fs_read::read_regular_file_to_string_limited(path, MAX_PROVIDER_SESSION_FILE_BYTES)?;
    let raw: Value = serde_json::from_str(&data)?;
    let raw_messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (messages, parse_stats) = convert_message_values(raw_messages);
    tracing::info!(
        path = %path.display(),
        raw_messages = parse_stats.records_seen,
        parsed_messages = messages.len(),
        parse_errors = parse_stats.parse_errors,
        skipped_records = parse_stats.skipped_records,
        empty_content = parse_stats.empty_content,
        "Gemini CLI message loading complete"
    );
    Ok(ProviderMessageLoad {
        messages,
        parse_stats,
    })
}

fn convert_message_values(raw_messages: Vec<Value>) -> (Vec<Message>, ProviderParseStats) {
    let mut messages = Vec::with_capacity(raw_messages.len());
    let mut parse_stats = ProviderParseStats::default();

    for (idx, raw) in raw_messages.into_iter().enumerate() {
        parse_stats.record_seen();
        let Ok(msg) = serde_json::from_value::<RawMessage>(raw) else {
            parse_stats.record_parse_error();
            continue;
        };

        let Some(role) = raw_role(msg.msg_type.as_ref()) else {
            parse_stats.record_skipped_record();
            continue;
        };

        let content = message_content(&msg, role);
        if content.is_empty() {
            parse_stats.record_empty_content();
            continue;
        }

        messages.push(message_from_content(&msg, role, content, idx));
    }

    (messages, parse_stats)
}

fn message_from_content(
    msg: &RawMessage,
    role: Role,
    content: Vec<ContentBlock>,
    fallback_idx: usize,
) -> Message {
    Message {
        id: MessageId(stringish(msg.id.as_ref(), &["id"]).unwrap_or_default()),
        role,
        timestamp: message_timestamp(msg.timestamp.as_ref(), fallback_idx),
        content,
        model: stringish(msg.model.as_ref(), &["model", "id", "name"]),
        token_usage: msg.tokens.as_ref().map(|tokens| {
            token_usage_from_options(
                value_u64(tokens.input.as_ref()),
                value_u64(tokens.output.as_ref()),
                value_u64(tokens.cached.as_ref()),
                None,
            )
        }),
    }
}

fn message_timestamp(raw: Option<&Value>, fallback_idx: usize) -> DateTime<Utc> {
    gemini_timestamp(raw).unwrap_or_else(|| epoch_timestamp_for_index(fallback_idx))
}
