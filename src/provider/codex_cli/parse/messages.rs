use std::path::Path;

use crate::model::Role;
use crate::provider::json_text::stringish;
use crate::provider::parse_common::visit_jsonl_records;
use crate::provider::{ProviderError, ProviderMessageLoad, ProviderParseStats};

use super::{entry_text, RawEntry};
use content::{legacy_content, push_event_msg, push_response_item};
use util::{assign_fallback_message_ids, entry_timestamp, error_message, message};

mod content;
mod util;

pub(crate) fn parse_rollout_messages_with_stats(
    path: &Path,
) -> Result<ProviderMessageLoad, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Codex CLI messages");
    let mut messages = Vec::new();
    let mut skipped_records: usize = 0;
    let mut empty_content: usize = 0;

    let stats = visit_jsonl_records::<RawEntry, _, _>(
        path,
        |record| {
            let line_number = record.line_number;
            let fallback_idx = line_number.saturating_sub(1);
            let entry = record.value;
            let entry_type = stringish(entry.entry_type.as_ref(), &["type"]).unwrap_or_default();
            let role = match entry_type.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool_use" => Role::Tool,
                "error" => {
                    if let Some(error_msg) = entry.error.as_ref().map(entry_text) {
                        messages.push(error_message(
                            entry_timestamp(&entry, fallback_idx),
                            error_msg,
                        ));
                    }
                    return;
                }
                "event_msg" => {
                    push_event_msg(&mut messages, &entry, fallback_idx);
                    return;
                }
                "response_item" => {
                    push_response_item(&mut messages, &entry, fallback_idx);
                    return;
                }
                _ => {
                    skipped_records += 1;
                    tracing::trace!(
                        entry_type = entry_type.as_str(),
                        "skipping non-message entry"
                    );
                    return;
                }
            };

            let timestamp = entry_timestamp(&entry, fallback_idx);
            let content = legacy_content(&entry, role);

            if content.is_empty() {
                empty_content += 1;
                tracing::trace!(
                    line_num = line_number,
                    entry_type = entry_type.as_str(),
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
        skipped_records,
        empty_content,
        messages = messages.len(),
        "Codex CLI message loading complete"
    );

    assign_fallback_message_ids(&mut messages);

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
