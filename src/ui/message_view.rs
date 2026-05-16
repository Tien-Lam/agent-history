use std::collections::HashMap;
use std::fmt::Write as _;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{ContentBlock, Message, Role, Session};
use crate::ui::{border_style, palette, role_style, syntax};

mod tools;

use tools::{collect_tool_outcomes, preview_limit, push_collapsed_tool_details};

/// Args/output truncation when the raw toggle is OFF. Keeps long shell
/// output and giant JSON args from drowning the panel; press `r` to bypass.
const ARGS_PREVIEW_LINES: usize = 20;
const OUTPUT_PREVIEW_LINES: usize = 10;
const THINKING_PREVIEW_LINES: usize = 5;

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

    pub fn render(
        &self,
        session: Option<&Session>,
        messages: Option<&[Message]>,
        focused: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let block = message_view_block(session, self.scroll_offset, focused);

        let Some(messages) = messages else {
            render_placeholder(
                frame,
                area,
                block,
                "Select a session to view the conversation",
            );
            return;
        };

        if messages.is_empty() {
            render_placeholder(frame, area, block, "No messages in this session");
            return;
        }

        let lines = self.message_lines(messages);

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        frame.render_widget(paragraph, area);
    }

    fn message_lines(&self, messages: &[Message]) -> Vec<Line<'static>> {
        let outcomes = collect_tool_outcomes(messages);
        let mut lines = Vec::new();
        for msg in messages {
            if is_tool_result_echo(msg) {
                continue;
            }
            push_message_header(&mut lines, msg);
            for content_block in &msg.content {
                self.push_content_block(&mut lines, content_block, &outcomes);
            }
            lines.push(Line::raw(""));
        }
        lines
    }

    fn push_content_block(
        &self,
        lines: &mut Vec<Line<'static>>,
        content_block: &ContentBlock,
        outcomes: &HashMap<String, bool>,
    ) {
        match content_block {
            ContentBlock::Text(text) => push_text(lines, text),
            ContentBlock::CodeBlock { language, code } => {
                push_code_block(lines, language.as_deref(), code);
            }
            ContentBlock::ToolUse(tool_call) => self.push_tool_use(lines, tool_call, outcomes),
            ContentBlock::ToolResult(result) => self.push_tool_result(lines, result),
            ContentBlock::Thinking(text) => self.push_thinking(lines, text),
            ContentBlock::Error(text) => push_error(lines, text),
        }
    }

    fn push_tool_use(
        &self,
        lines: &mut Vec<Line<'static>>,
        tool_call: &crate::model::ToolCall,
        outcomes: &HashMap<String, bool>,
    ) {
        let marker = if self.show_tool_calls {
            "\u{25bc}"
        } else {
            "\u{25b6}"
        };
        let mut header = vec![
            Span::styled(format!(" {marker} "), Style::default().fg(palette::YELLOW)),
            Span::styled(
                tool_call.name.clone(),
                Style::default()
                    .fg(palette::YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if !self.show_tool_calls {
            push_collapsed_tool_details(&mut header, tool_call, outcomes);
        }
        lines.push(Line::from(header));

        if self.show_tool_calls {
            let limit = preview_limit(self.show_raw_output, ARGS_PREVIEW_LINES);
            for arg_line in tool_call.arguments.lines().take(limit) {
                lines.push(dim_line(format!("   {arg_line}")));
            }
        }
    }

    fn push_tool_result(&self, lines: &mut Vec<Line<'static>>, result: &crate::model::ToolResult) {
        if !self.show_tool_calls {
            return;
        }
        let (status, color) = if result.success {
            ("\u{2714} ok", palette::GREEN)
        } else {
            ("\u{2718} error", palette::RED)
        };
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                status,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
        let limit = preview_limit(self.show_raw_output, OUTPUT_PREVIEW_LINES);
        let output_lines: Vec<&str> = result.output.lines().collect();
        let shown = output_lines.len().min(limit);
        for out_line in output_lines.iter().take(shown) {
            lines.push(dim_line(format!("   {out_line}")));
        }
        if output_lines.len() > shown {
            lines.push(Line::from(Span::styled(
                format!(
                    "   \u{2026} {} more line(s) — press r for raw",
                    output_lines.len() - shown
                ),
                Style::default()
                    .fg(palette::TEXT_FAINT)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }

    fn push_thinking(&self, lines: &mut Vec<Line<'static>>, text: &str) {
        if !self.show_tool_calls || text.is_empty() {
            return;
        }
        lines.push(Line::from(Span::styled(
            " \u{1f4ad} Thinking",
            Style::default()
                .fg(palette::MAUVE)
                .add_modifier(Modifier::ITALIC),
        )));
        let limit = preview_limit(self.show_raw_output, THINKING_PREVIEW_LINES);
        for thought_line in text.lines().take(limit) {
            lines.push(Line::from(Span::styled(
                format!("   {thought_line}"),
                Style::default()
                    .fg(palette::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
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

fn message_view_block(
    session: Option<&Session>,
    scroll_offset: u16,
    focused: bool,
) -> Block<'static> {
    let title = session.map_or_else(|| " No session selected ".to_string(), session_title);
    let scroll_indicator = if scroll_offset > 0 {
        format!(" \u{2191}{scroll_offset} ")
    } else {
        String::new()
    };

    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(
            Line::from(scroll_indicator)
                .style(Style::default().fg(palette::TEXT_DIM))
                .right_aligned(),
        )
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style(focused))
}

fn session_title(session: &Session) -> String {
    let project = session.project_name.as_deref().unwrap_or("Session");
    let model = session.model.as_deref().unwrap_or("unknown");
    let mut parts = format!(" {project} \u{2022} {model}");
    if let Some(ref usage) = session.token_usage {
        let total = usage.input_tokens + usage.output_tokens;
        if total > 0 {
            let _ = write!(parts, " \u{2022} {}k tok", total / 1000);
        }
    }
    parts.push(' ');
    parts
}

fn render_placeholder(frame: &mut Frame, area: Rect, block: Block<'static>, message: &'static str) {
    let placeholder = Paragraph::new(Line::from(Span::styled(
        message,
        Style::default().fg(palette::TEXT_DIM),
    )))
    .block(block);
    frame.render_widget(placeholder, area);
}

fn is_tool_result_echo(msg: &Message) -> bool {
    msg.role == Role::User
        && msg
            .content
            .iter()
            .all(|c| matches!(c, ContentBlock::ToolResult(_)))
}

fn push_message_header(lines: &mut Vec<Line<'static>>, msg: &Message) {
    let time_str = msg.timestamp.format("%H:%M:%S").to_string();
    let role_label = format!(" {} ", msg.role);
    lines.push(Line::from(vec![
        Span::styled(
            role_label,
            role_style(msg.role).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(time_str, Style::default().fg(palette::TEXT_FAINT)),
    ]));
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(40),
        Style::default().fg(palette::TEXT_FAINT),
    )));
}

fn push_text(lines: &mut Vec<Line<'static>>, text: &str) {
    for text_line in text.lines() {
        lines.push(Line::from(Span::styled(
            format!(" {text_line}"),
            Style::default().fg(palette::TEXT),
        )));
    }
}

fn push_code_block(lines: &mut Vec<Line<'static>>, language: Option<&str>, code: &str) {
    let lang_label = language.unwrap_or("code");
    lines.push(Line::from(Span::styled(
        format!(" \u{256d}\u{2500} {lang_label} \u{2500}\u{2500}\u{2500}"),
        Style::default().fg(palette::TEXT_FAINT),
    )));
    for code_line in code.lines() {
        let mut spans = vec![Span::styled(
            " \u{2502} ",
            Style::default().fg(palette::TEXT_FAINT),
        )];
        spans.extend(syntax::highlight_line(language, code_line));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        " \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        Style::default().fg(palette::TEXT_FAINT),
    )));
}

fn push_error(lines: &mut Vec<Line<'static>>, text: &str) {
    lines.push(Line::from(Span::styled(
        format!(" \u{2718} Error: {text}"),
        Style::default()
            .fg(palette::RED)
            .add_modifier(Modifier::BOLD),
    )));
}

fn dim_line(text: String) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(palette::TEXT_DIM)))
}
