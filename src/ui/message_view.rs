use std::collections::HashMap;
use std::fmt::Write as _;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{ContentBlock, Message, Role, Session};
use crate::ui::{border_style, palette, role_style, syntax};

/// Args/output truncation when the raw toggle is OFF. Keeps long shell
/// output and giant JSON args from drowning the panel; press `r` to bypass.
const ARGS_PREVIEW_LINES: usize = 20;
const OUTPUT_PREVIEW_LINES: usize = 10;
const THINKING_PREVIEW_LINES: usize = 5;

/// Inline-arg-summary cap when a tool call is collapsed. Just enough to
/// distinguish "Read main.rs" from "Read foo.txt" without wrapping.
const COLLAPSED_ARG_CHARS: usize = 60;

pub struct MessageViewComponent {
    pub scroll_offset: u16,
    /// `false` = collapsed (tool name + arg summary + status badge);
    /// `true`  = expanded (args + output + thinking visible).
    pub show_tool_calls: bool,
    /// When `true`, drop the `ARGS_PREVIEW_LINES` / `OUTPUT_PREVIEW_LINES`
    /// caps so users can read full tool I/O. Bound to `r` in view mode.
    pub show_raw_output: bool,
}

impl Default for MessageViewComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageViewComponent {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            show_tool_calls: false,
            show_raw_output: false,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn render(
        &self,
        session: Option<&Session>,
        messages: Option<&[Message]>,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let title = session.map_or_else(
            || " No session selected ".to_string(),
            |s| {
                let project = s.project_name.as_deref().unwrap_or("Session");
                let model = s.model.as_deref().unwrap_or("unknown");
                let mut parts = format!(" {project} \u{2022} {model}");
                if let Some(ref usage) = s.token_usage {
                    let total = usage.input_tokens + usage.output_tokens;
                    if total > 0 {
                        let _ = write!(parts, " \u{2022} {}k tok", total / 1000);
                    }
                }
                parts.push(' ');
                parts
            },
        );

        let scroll_indicator = if self.scroll_offset > 0 {
            format!(" \u{2191}{} ", self.scroll_offset)
        } else {
            String::new()
        };

        let block = Block::default()
            .title(title)
            .title_style(Style::default().fg(palette::TEXT).add_modifier(Modifier::BOLD))
            .title_bottom(
                Line::from(scroll_indicator)
                    .style(Style::default().fg(palette::TEXT_DIM))
                    .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style(focused));

        let Some(messages) = messages else {
            let placeholder = Paragraph::new(Line::from(Span::styled(
                "Select a session to view the conversation",
                Style::default().fg(palette::TEXT_DIM),
            )))
            .block(block);
            frame.render_widget(placeholder, area);
            return;
        };

        if messages.is_empty() {
            let placeholder = Paragraph::new(Line::from(Span::styled(
                "No messages in this session",
                Style::default().fg(palette::TEXT_DIM),
            )))
            .block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        // Pre-scan for tool result outcomes so collapsed tool-call lines can
        // show ✓/✗ next to the name without expanding the user. Keyed by
        // tool_call_id; tool results that arrive before the call (rare) are
        // still picked up because the lookup is global.
        let outcomes = collect_tool_outcomes(messages);

        let mut lines: Vec<Line> = Vec::new();

        for msg in messages {
            if msg.role == Role::User
                && msg
                    .content
                    .iter()
                    .all(|c| matches!(c, ContentBlock::ToolResult(_)))
            {
                continue;
            }

            // Role header with separator
            let time_str = msg.timestamp.format("%H:%M:%S").to_string();
            let role_label = format!(" {} ", msg.role);
            lines.push(Line::from(vec![
                Span::styled(
                    role_label,
                    role_style(msg.role)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(time_str, Style::default().fg(palette::TEXT_FAINT)),
            ]));
            // Thin separator after header
            lines.push(Line::from(Span::styled(
                "\u{2500}".repeat(40),
                Style::default().fg(palette::TEXT_FAINT),
            )));

            for content_block in &msg.content {
                match content_block {
                    ContentBlock::Text(text) => {
                        for text_line in text.lines() {
                            lines.push(Line::from(Span::styled(
                                format!(" {text_line}"),
                                Style::default().fg(palette::TEXT),
                            )));
                        }
                    }
                    ContentBlock::CodeBlock { language, code } => {
                        let lang_label = language.as_deref().unwrap_or("code");
                        lines.push(Line::from(Span::styled(
                            format!(" \u{256d}\u{2500} {lang_label} \u{2500}\u{2500}\u{2500}"),
                            Style::default().fg(palette::TEXT_FAINT),
                        )));
                        for code_line in code.lines() {
                            let mut spans = vec![Span::styled(
                                " \u{2502} ",
                                Style::default().fg(palette::TEXT_FAINT),
                            )];
                            spans.extend(syntax::highlight_line(language.as_deref(), code_line));
                            lines.push(Line::from(spans));
                        }
                        lines.push(Line::from(Span::styled(
                            " \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                            Style::default().fg(palette::TEXT_FAINT),
                        )));
                    }
                    ContentBlock::ToolUse(tool_call) => {
                        let marker = if self.show_tool_calls {
                            "\u{25bc}" // ▼ expanded
                        } else {
                            "\u{25b6}" // ▶ collapsed
                        };
                        let mut header = vec![
                            Span::styled(
                                format!(" {marker} "),
                                Style::default().fg(palette::YELLOW),
                            ),
                            Span::styled(
                                tool_call.name.clone(),
                                Style::default()
                                    .fg(palette::YELLOW)
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ];
                        if !self.show_tool_calls {
                            // Collapsed: show a one-line arg summary and the
                            // result outcome so users can scan without expanding.
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
                                    Style::default()
                                        .fg(color)
                                        .add_modifier(Modifier::BOLD),
                                ));
                            }
                        }
                        lines.push(Line::from(header));

                        if self.show_tool_calls {
                            let limit = if self.show_raw_output {
                                usize::MAX
                            } else {
                                ARGS_PREVIEW_LINES
                            };
                            for arg_line in tool_call.arguments.lines().take(limit) {
                                lines.push(Line::from(Span::styled(
                                    format!("   {arg_line}"),
                                    Style::default().fg(palette::TEXT_DIM),
                                )));
                            }
                        }
                    }
                    ContentBlock::ToolResult(result) => {
                        if self.show_tool_calls {
                            let (status, color) = if result.success {
                                ("\u{2714} ok", palette::GREEN)
                            } else {
                                ("\u{2718} error", palette::RED)
                            };
                            lines.push(Line::from(vec![
                                Span::styled("   ", Style::default()),
                                Span::styled(
                                    status,
                                    Style::default()
                                        .fg(color)
                                        .add_modifier(Modifier::BOLD),
                                ),
                            ]));
                            let limit = if self.show_raw_output {
                                usize::MAX
                            } else {
                                OUTPUT_PREVIEW_LINES
                            };
                            let output_lines: Vec<&str> = result.output.lines().collect();
                            let total = output_lines.len();
                            let shown = total.min(limit);
                            for out_line in output_lines.iter().take(shown) {
                                lines.push(Line::from(Span::styled(
                                    format!("   {out_line}"),
                                    Style::default().fg(palette::TEXT_DIM),
                                )));
                            }
                            if total > shown {
                                lines.push(Line::from(Span::styled(
                                    format!(
                                        "   \u{2026} {} more line(s) — press r for raw",
                                        total - shown
                                    ),
                                    Style::default()
                                        .fg(palette::TEXT_FAINT)
                                        .add_modifier(Modifier::ITALIC),
                                )));
                            }
                        }
                    }
                    ContentBlock::Thinking(text) => {
                        if self.show_tool_calls && !text.is_empty() {
                            lines.push(Line::from(Span::styled(
                                " \u{1f4ad} Thinking",
                                Style::default()
                                    .fg(palette::MAUVE)
                                    .add_modifier(Modifier::ITALIC),
                            )));
                            let limit = if self.show_raw_output {
                                usize::MAX
                            } else {
                                THINKING_PREVIEW_LINES
                            };
                            for thought_line in text.lines().take(limit) {
                                lines.push(Line::from(Span::styled(
                                    format!("   {thought_line}"),
                                    Style::default()
                                        .fg(palette::TEXT_DIM)
                                        .add_modifier(Modifier::ITALIC),
                                )));
                            }
                        }
                    }
                    ContentBlock::Error(text) => {
                        lines.push(Line::from(Span::styled(
                            format!(" \u{2718} Error: {text}"),
                            Style::default()
                                .fg(palette::RED)
                                .add_modifier(Modifier::BOLD),
                        )));
                    }
                }
            }

            // Spacing between messages
            lines.push(Line::raw(""));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        frame.render_widget(paragraph, area);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }
}

/// Walk every tool result in the session and map its `tool_call_id` to the
/// success bit. Used to draw ✓/✗ on collapsed tool-call headers without
/// holding the per-message borrow across loop iterations.
fn collect_tool_outcomes(messages: &[Message]) -> HashMap<String, bool> {
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
/// collapsed header. Tries common JSON keys (`file_path`, `path`, `command`, …)
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
                "file_path", "path", "filename", "file",
                "command", "cmd", "query", "pattern",
                "url", "name", "description",
            ];
            for key in PREFERRED_KEYS {
                if let Some(v) = obj.get(*key) {
                    if let Some(s) = json_value_to_brief(v) {
                        return Some(truncate_for_header(&s));
                    }
                }
            }
            // No preferred key — show the first short string field so users
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
mod tests {
    use super::*;

    #[test]
    fn collapsed_summary_prefers_file_path() {
        let s = collapsed_arg_summary(r#"{"file_path":"src/main.rs","limit":50}"#).unwrap();
        assert_eq!(s, "src/main.rs");
    }

    #[test]
    fn collapsed_summary_uses_command_for_shell() {
        let s = collapsed_arg_summary(r#"{"command":"ls -la","description":"List files"}"#).unwrap();
        assert_eq!(s, "ls -la");
    }

    #[test]
    fn collapsed_summary_truncates_long_values() {
        let long = "a".repeat(200);
        let json = format!(r#"{{"path":"{long}"}}"#);
        let s = collapsed_arg_summary(&json).unwrap();
        // Should end with the ellipsis and stay within the budget.
        assert!(s.ends_with('\u{2026}'));
        assert!(s.chars().count() <= COLLAPSED_ARG_CHARS + 1);
    }

    #[test]
    fn collapsed_summary_falls_back_to_first_line_for_non_json() {
        let s = collapsed_arg_summary("first line\nsecond line").unwrap();
        assert_eq!(s, "first line");
    }

    #[test]
    fn collapsed_summary_returns_none_for_empty() {
        assert!(collapsed_arg_summary("").is_none());
        assert!(collapsed_arg_summary("   \n  ").is_none());
    }

    #[test]
    fn collapsed_summary_skips_object_only_args() {
        // No string/number/bool fields → nothing useful to surface.
        assert!(collapsed_arg_summary(r#"{"nested":{"k":1}}"#).is_none());
    }
}
