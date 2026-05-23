use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Message, MessageId, Role};
use crate::provider::anthropic_content::{content_to_blocks, AnthropicContent};
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{timestamp_with_index_millis, visit_jsonl_records};
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

#[derive(Deserialize)]
struct SessionLine {
    role: Option<Value>,
    #[serde(default)]
    content: AnthropicContent,
}

pub(crate) fn parse_jsonl(path: &Path, base_ts: &DateTime<Utc>) -> Result<Vec<Message>, String> {
    Ok(parse_jsonl_with_stats(path, base_ts)?.messages)
}

pub(crate) fn parse_jsonl_with_stats(
    path: &Path,
    base_ts: &DateTime<Utc>,
) -> Result<ProviderMessageLoad, String> {
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<SessionLine, _, _>(
        path,
        |record| {
            let idx = record.line_number.saturating_sub(1);
            let parsed = record.value;
            let role = match stringish(parsed.role.as_ref(), &["role", "type"]).as_deref() {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                Some("system") => Role::System,
                _ => {
                    skipped_records += 1;
                    return;
                }
            };

            let blocks = content_to_blocks(parsed.content);
            if blocks.is_empty() {
                empty_content += 1;
                return;
            }

            let timestamp = timestamp_with_index_millis(*base_ts, idx);

            messages.push(Message {
                id: MessageId(format!("msg-{idx}")),
                role,
                timestamp,
                content: blocks,
                model: None,
                token_usage: None,
            });
        },
        |error| {
            tracing::warn!(line_num = error.line_number, error = %error.error, "failed to parse Continue JSONL line");
        },
    )
    .map_err(|e| e.to_string())?;

    tracing::info!(
        path = %path.display(),
        lines = stats.line_count,
        parse_errors = stats.parse_errors,
        skipped_records,
        empty_content,
        messages = messages.len(),
        "Continue.dev message loading complete"
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
