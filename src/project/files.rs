use std::collections::HashMap;

use serde::Serialize;

use crate::model::{ContentBlock, Message, Session, ToolCall};

/// One row per touched file path. `count` is the number of tool calls
/// referencing the path (Read/Edit/Write/etc.).
#[derive(Debug, Clone, Serialize)]
pub struct FileTouch {
    pub path: String,
    pub count: u64,
}

/// Tool-call argument keys that name a file path. Matched at the top level
/// of the parsed JSON object; nested edit lists (e.g. `MultiEdit.edits[]`)
/// are not walked. This is intentional: top-level keys cover the providers'
/// canonical Read/Edit/Write/Bash/Notebook* shapes without false positives
/// from arbitrary user-supplied JSON inside `arguments`.
const FILE_PATH_KEYS: &[&str] = &[
    "file_path",
    "path",
    "notebook_path",
    "filename",
    "target_file",
];

pub(super) fn top_files(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
) -> (usize, Vec<FileTouch>) {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for (_, msgs) in sessions {
        for msg in msgs {
            for block in &msg.content {
                if let ContentBlock::ToolUse(tool) = block {
                    if let Some(path) = extract_file_path(tool) {
                        *counts.entry(path).or_default() += 1;
                    }
                }
            }
        }
    }
    let mut rows: Vec<FileTouch> = counts
        .into_iter()
        .map(|(path, count)| FileTouch { path, count })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
    let total = rows.len();
    if limit > 0 && rows.len() > limit {
        rows.truncate(limit);
    }
    (total, rows)
}

fn extract_file_path(tool: &ToolCall) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&tool.arguments).ok()?;
    let obj = v.as_object()?;
    for key in FILE_PATH_KEYS {
        if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}
