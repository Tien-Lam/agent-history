use std::path::Path;

use crate::fs_read;
use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{stringish, value_u64};
use crate::provider::parse_common::{
    file_modified_utc, token_usage_from_options, unix_epoch_utc, MAX_PROVIDER_SESSION_FILE_BYTES,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::ProviderParseStats;

use super::{message_text, timestamp_from_values, RawMessage};

mod parts;

use parts::load_parts_into_content;

pub(crate) fn message_id_from_file(path: &Path) -> Option<String> {
    let data =
        fs_read::read_regular_file_to_string_limited(path, MAX_PROVIDER_SESSION_FILE_BYTES).ok()?;
    let raw = serde_json::from_str::<RawMessage>(&data).ok()?;
    stringish(raw.id.as_ref(), &["id"])
}

pub(crate) fn parse_message_file_with_stats(
    path: &Path,
    part_dir: &Path,
    parse_stats: &mut ProviderParseStats,
) -> Option<Message> {
    parse_stats.record_seen();
    match parse_message_file_outcome(path, part_dir) {
        MessageFileOutcome::Message(message) => Some(message),
        MessageFileOutcome::ParseError => {
            parse_stats.record_parse_error();
            None
        }
        MessageFileOutcome::SkippedRecord => {
            parse_stats.record_skipped_record();
            None
        }
        MessageFileOutcome::EmptyContent => {
            parse_stats.record_empty_content();
            None
        }
    }
}

enum MessageFileOutcome {
    Message(Message),
    ParseError,
    SkippedRecord,
    EmptyContent,
}

fn parse_message_file_outcome(path: &Path, part_dir: &Path) -> MessageFileOutcome {
    let Ok(data) =
        fs_read::read_regular_file_to_string_limited(path, MAX_PROVIDER_SESSION_FILE_BYTES)
    else {
        return MessageFileOutcome::ParseError;
    };
    let Ok(raw) = serde_json::from_str::<RawMessage>(&data) else {
        return MessageFileOutcome::ParseError;
    };

    let role = match stringish(raw.role.as_ref(), &["role", "type"]).as_deref() {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return MessageFileOutcome::SkippedRecord,
    };

    // Try new format (time.created as millis) first, then legacy (timestamp as ISO string)
    let timestamp = timestamp_from_values(
        raw.time.as_ref().and_then(|t| t.created.as_ref()),
        raw.timestamp.as_ref(),
    )
    .or_else(|| file_modified_utc(path))
    .unwrap_or_else(unix_epoch_utc);

    let msg_id = stringish(raw.id.as_ref(), &["id"]).unwrap_or_default();
    let mut content = Vec::new();

    // Try loading parts from part/{messageID}/ directory (new format)
    let msg_part_dir = part_dir.join(&msg_id);
    if msg_part_dir.exists() {
        load_parts_into_content(&msg_part_dir, &mut content);
    }

    // Fall back to legacy fields if no parts found
    if content.is_empty() {
        if let Some(text) = raw.content.as_ref().map(message_text) {
            if !text.is_empty() {
                content.extend(parse_text_with_code_blocks(&text));
            }
        }

        if let Some(changes) = &raw.code_changes {
            for change in changes {
                let label = change
                    .path
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["path", "file"]))
                    .unwrap_or_else(|| "diff".to_string());
                let diff = change.diff.as_ref().map(message_text).unwrap_or_default();
                if !diff.is_empty() {
                    content.push(ContentBlock::CodeBlock {
                        language: Some(format!("diff ({label})")),
                        code: diff,
                    });
                }
            }
        }
    }

    // If still no content, try summary.title (new format user messages)
    if content.is_empty() {
        if let Some(ref summary) = raw.summary {
            if let Some(title) = summary.title.as_ref().map(message_text) {
                if !title.is_empty() {
                    content.push(ContentBlock::Text(title));
                }
            }
        }
    }

    if content.is_empty() {
        return MessageFileOutcome::EmptyContent;
    }

    let token_usage = raw.tokens.as_ref().map(|t| {
        token_usage_from_options(
            value_u64(t.input.as_ref()),
            value_u64(t.output.as_ref()),
            t.cache.as_ref().and_then(|c| value_u64(c.read.as_ref())),
            t.cache.as_ref().and_then(|c| value_u64(c.write.as_ref())),
        )
    });

    let model = raw
        .model
        .and_then(|m| stringish(m.model_id.as_ref(), &["modelID", "model", "id"]));

    MessageFileOutcome::Message(Message {
        id: MessageId(msg_id),
        role,
        timestamp,
        content,
        model,
        token_usage,
    })
}
