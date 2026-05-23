use std::collections::HashMap;

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::model::{ContentBlock, Message, ToolCall};
use crate::ui::palette;

/// Inline-arg-summary cap when a tool call is collapsed. Just enough to
/// distinguish "Read main.rs" from "Read foo.txt" without wrapping.
const COLLAPSED_ARG_CHARS: usize = 60;

pub(super) fn push_collapsed_tool_details(
    header: &mut Vec<Span<'static>>,
    tool_call: &ToolCall,
    outcomes: &HashMap<String, bool>,
) {
    if let Some(summary) = collapsed_arg_summary(&tool_call.arguments) {
        header.push(Span::styled(
            format!(" {summary}"),
            Style::default().fg(palette::TEXT_DIM),
        ));
    }
    if let Some(success) = outcomes.get(&tool_call.id) {
        let (badge, color) = if *success {
            (" \u{2714}", palette::GREEN)
        } else {
            (" \u{2718}", palette::RED)
        };
        header.push(Span::styled(
            badge,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
}

pub(super) fn preview_limit(show_raw_output: bool, collapsed_limit: usize) -> usize {
    if show_raw_output {
        usize::MAX
    } else {
        collapsed_limit
    }
}

/// Walk every tool result in the session and map its `tool_call_id` to the
/// success bit. Used to draw status badges on collapsed tool-call headers
/// without holding the per-message borrow across loop iterations.
pub(super) fn collect_tool_outcomes(messages: &[Message]) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for msg in messages {
        for block in &msg.content {
            if let ContentBlock::ToolResult(result) = block {
                out.insert(result.tool_call_id.clone(), result.success);
            }
        }
    }
    out
}

/// Best-effort one-line summary of a tool call's arguments, for the
/// collapsed header. Tries common JSON keys (`file_path`, `path`, `command`, ...)
/// before falling back to the first non-empty line. Returns `None` when
/// the args are empty or yield nothing useful.
fn collapsed_arg_summary(args: &str) -> Option<String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(obj) = value.as_object() {
            const PREFERRED_KEYS: &[&str] = &[
                "file_path",
                "path",
                "filename",
                "file",
                "command",
                "cmd",
                "query",
                "pattern",
                "url",
                "name",
                "description",
            ];
            for key in PREFERRED_KEYS {
                if let Some(v) = obj.get(*key) {
                    if let Some(s) = json_value_to_brief(v) {
                        return Some(truncate_for_header(&s));
                    }
                }
            }
            // No preferred key: show the first short string field so users
            // still get a hint about what the call was for.
            for v in obj.values() {
                if let Some(s) = json_value_to_brief(v) {
                    return Some(truncate_for_header(&s));
                }
            }
            return None;
        }
        if let Some(s) = json_value_to_brief(&value) {
            return Some(truncate_for_header(&s));
        }
    }

    let first = trimmed.lines().find(|l| !l.trim().is_empty())?;
    Some(truncate_for_header(first.trim()))
}

fn json_value_to_brief(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn truncate_for_header(s: &str) -> String {
    let single_line: String = s
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if single_line.chars().count() > COLLAPSED_ARG_CHARS {
        let truncated: String = single_line.chars().take(COLLAPSED_ARG_CHARS).collect();
        format!("{truncated}\u{2026}")
    } else {
        single_line
    }
}

#[cfg(test)]
mod tests;
