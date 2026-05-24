use std::path::Path;

use self::content::parse_message_content;
use super::record::{claude_timestamp, RawSessionEntry};
use super::ProviderError;
use crate::model::{Message, MessageId, Role};
use crate::provider::json_text::{stringish, value_u64};
use crate::provider::parse_common::{
    epoch_timestamp_for_index, token_usage_from_options, visit_jsonl_records,
};
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

mod content;

pub(crate) fn parse_session_messages(path: &Path) -> Result<Vec<Message>, ProviderError> {
    Ok(parse_session_messages_with_stats(path)?.messages)
}

pub(crate) fn parse_session_messages_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Claude Code messages");
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawSessionEntry, _, _>(
        path,
        |record| {
            let line_number = record.line_number;
            let entry = record.value;
            let role_text = stringish(entry.entry_type.as_ref(), &["type"]);
            let role = match role_text.as_deref() {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                Some(other) => {
                    skipped_records += 1;
                    tracing::trace!(entry_type = other, "skipping non-message entry");
                    return;
                }
                None => {
                    skipped_records += 1;
                    return;
                }
            };

            let Some(ref msg) = entry.message else {
                skipped_records += 1;
                tracing::warn!(line_num = line_number, role = ?role, uuid = ?entry.uuid, "entry has no message field");
                return;
            };

            let timestamp = claude_timestamp(entry.timestamp.as_ref())
                .unwrap_or_else(|| epoch_timestamp_for_index(line_number.saturating_sub(1)));

            let id = stringish(entry.uuid.as_ref(), &["uuid", "id"]).unwrap_or_default();

            let content = parse_message_content(msg, role);
            if content.is_empty() {
                empty_content += 1;
                tracing::trace!(line_num = line_number, msg_id = %id, role = ?role, "skipping message with empty content");
                return;
            }

            let token_usage = msg.usage.as_ref().map(|u| {
                token_usage_from_options(
                    value_u64(u.input_tokens.as_ref()),
                    value_u64(u.output_tokens.as_ref()),
                    value_u64(u.cache_read_input_tokens.as_ref()),
                    value_u64(u.cache_creation_input_tokens.as_ref()),
                )
            });

            messages.push(Message {
                id: MessageId(id),
                role,
                timestamp,
                content,
                model: stringish(msg.model.as_ref(), &["model", "id", "name"]),
                token_usage,
            });
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
        "Claude Code message loading complete"
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
