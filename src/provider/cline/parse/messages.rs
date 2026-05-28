use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::fs_read;
use crate::model::{Message, MessageId, Role};
use crate::provider::anthropic_content::{content_to_blocks, AnthropicContent};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{timestamp_with_index_millis, MAX_PROVIDER_SESSION_FILE_BYTES};
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

pub(crate) const API_HISTORY_FILE: &str = "api_conversation_history.json";

#[derive(Deserialize)]
struct ApiMessage {
    role: Option<Value>,
    #[serde(default)]
    content: AnthropicContent,
}

pub(super) fn parse_api_history(
    path: &Path,
    base_ts: &DateTime<Utc>,
) -> Result<Vec<Message>, String> {
    Ok(parse_api_history_with_stats(path, base_ts)?.messages)
}

pub(crate) fn parse_api_history_with_stats(
    path: &Path,
    base_ts: &DateTime<Utc>,
) -> Result<ProviderMessageLoad, String> {
    let bytes = fs_read::read_regular_file_limited(path, MAX_PROVIDER_SESSION_FILE_BYTES)
        .map_err(|e| format!("read: {e}"))?;
    let raw: Vec<Value> = serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;

    let mut messages = Vec::with_capacity(raw.len());
    let mut parse_stats = ProviderParseStats::default();
    for (idx, entry) in raw.into_iter().enumerate() {
        parse_stats.record_seen();
        let Ok(msg) = serde_json::from_value::<ApiMessage>(entry) else {
            parse_stats.record_parse_error();
            continue;
        };

        let role = match stringish(msg.role.as_ref(), &["role", "type"]).as_deref() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => {
                parse_stats.record_skipped_record();
                continue;
            }
        };

        let blocks = content_to_blocks(msg.content);
        if blocks.is_empty() {
            parse_stats.record_empty_content();
            continue;
        }

        // Spread messages 1 ms apart so ordering is stable even without embedded timestamps.
        let timestamp = timestamp_with_index_millis(*base_ts, idx);

        messages.push(Message {
            id: MessageId(format!("msg-{idx}")),
            role,
            timestamp,
            content: blocks,
            model: None,
            token_usage: None,
        });
    }

    Ok(ProviderMessageLoad {
        messages,
        parse_stats,
    })
}
