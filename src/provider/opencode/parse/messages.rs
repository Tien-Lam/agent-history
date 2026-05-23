use std::path::Path;

use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::json_text::{stringish, value_u64};
use crate::provider::parse_common::{
    pretty_json_opt, token_usage_from_options, tool_result_block, tool_use_block,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;
use crate::provider::ProviderParseStats;

use super::{message_text, timestamp_from_values, tool_output_text, RawMessage, RawPart};

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
    let Ok(data) = std::fs::read_to_string(path) else {
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
    .unwrap_or_else(chrono::Utc::now);

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

/// Load content blocks from part files in a message's part directory.
fn load_parts_into_content(part_dir: &Path, content: &mut Vec<ContentBlock>) {
    let Ok(entries) = std::fs::read_dir(part_dir) else {
        return;
    };

    let mut parts: Vec<(String, RawPart)> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!(part_dir = %part_dir.display(), error = %e, "skipping unreadable OpenCode part entry");
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let Ok(data) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(part) = serde_json::from_str::<RawPart>(&data) else {
            continue;
        };

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        parts.push((filename, part));
    }

    // Sort by filename (part IDs are roughly chronological)
    parts.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, part) in &parts {
        match stringish(part.part_type.as_ref(), &["type"])
            .as_deref()
            .unwrap_or("")
        {
            "text" => {
                if let Some(text) = part.text.as_ref().map(message_text) {
                    if !text.is_empty() {
                        content.extend(parse_text_with_code_blocks(&text));
                    }
                }
            }
            "tool" => {
                let tool_name = part
                    .tool
                    .as_ref()
                    .and_then(|value| stringish(Some(value), &["name", "tool"]))
                    .unwrap_or_else(|| "unknown".to_string());
                let call_id =
                    stringish(part.call_id.as_ref(), &["callID", "id"]).unwrap_or_default();
                let arguments = pretty_json_opt(part.state.as_ref().and_then(|s| s.input.as_ref()));
                content.push(tool_use_block(call_id, tool_name, arguments));

                // Include tool output as a result
                if let Some(ref state) = part.state {
                    if let Some(output) = state.output.as_ref().map(tool_output_text) {
                        if !output.is_empty() {
                            let tool_call_id = stringish(part.call_id.as_ref(), &["callID", "id"])
                                .unwrap_or_default();
                            let success = stringish(state.status.as_ref(), &["status", "state"])
                                .as_deref()
                                == Some("completed");
                            content.push(tool_result_block(tool_call_id, success, output));
                        }
                    }
                }
            }
            // Skip step-start, step-finish, and other structural types
            _ => {}
        }
    }
}
