use std::path::Path;

use crate::fs_read;
use crate::model::ContentBlock;
use crate::provider::json_text::stringish;
use crate::provider::parse_common::{
    pretty_json_opt, tool_result_block, tool_use_block, MAX_PROVIDER_SESSION_FILE_BYTES,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

use super::super::{message_text, tool_output_text, RawPart};

/// Load content blocks from part files in a message's part directory.
pub(super) fn load_parts_into_content(part_dir: &Path, content: &mut Vec<ContentBlock>) {
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

        let Ok(data) = fs_read::read_to_string_limited(&path, MAX_PROVIDER_SESSION_FILE_BYTES)
        else {
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
