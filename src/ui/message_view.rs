use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{ContentBlock, Message, Role, Session};
use crate::ui::{border_style, palette, role_style};

pub struct MessageViewComponent {
    pub scroll_offset: u16,
    pub show_tool_calls: bool,
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
                format!(" {project} \u{2022} {model} ")
            },
        );

        let block = Block::default()
            .title(title)
            .title_style(Style::default().fg(palette::TEXT).add_modifier(Modifier::BOLD))
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
                            lines.push(Line::from(vec![
                                Span::styled(
                                    " \u{2502} ",
                                    Style::default().fg(palette::TEXT_FAINT),
                                ),
                                Span::styled(
                                    code_line.to_string(),
                                    Style::default().fg(palette::TEAL),
                                ),
                            ]));
                        }
                        lines.push(Line::from(Span::styled(
                            " \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                            Style::default().fg(palette::TEXT_FAINT),
                        )));
                    }
                    ContentBlock::ToolUse(tool_call) => {
                        let marker = if self.show_tool_calls {
                            "\u{25bc}"
                        } else {
                            "\u{25b6}"
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!(" {marker} "),
                                Style::default().fg(palette::YELLOW),
                            ),
                            Span::styled(
                                &tool_call.name,
                                Style::default()
                                    .fg(palette::YELLOW)
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]));
                        if self.show_tool_calls {
                            for arg_line in tool_call.arguments.lines().take(20) {
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
                            for out_line in result.output.lines().take(10) {
                                lines.push(Line::from(Span::styled(
                                    format!("   {out_line}"),
                                    Style::default().fg(palette::TEXT_DIM),
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
                            for thought_line in text.lines().take(5) {
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
