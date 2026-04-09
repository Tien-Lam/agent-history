use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{ContentBlock, Message, Role, Session};
use crate::ui::role_style;

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
        let border_style = if focused {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = session.map_or_else(|| " No session selected ".to_string(), |s| {
            let project = s.project_name.as_deref().unwrap_or("Session");
            let model = s.model.as_deref().unwrap_or("unknown");
            format!(" {project} ({model}) ")
        });

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let Some(messages) = messages else {
            let placeholder =
                Paragraph::new("Select a session to view the conversation").block(block);
            frame.render_widget(placeholder, area);
            return;
        };

        if messages.is_empty() {
            let placeholder = Paragraph::new("No messages in this session").block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        for msg in messages {
            // Skip tool results in user messages — they're shown with the tool call
            if msg.role == Role::User
                && msg
                    .content
                    .iter()
                    .all(|c| matches!(c, ContentBlock::ToolResult(_)))
            {
                continue;
            }

            // Role header
            let time_str = msg.timestamp.format("%H:%M:%S").to_string();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}]", msg.role),
                    role_style(msg.role).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(time_str, Style::default().fg(Color::DarkGray)),
            ]));

            for block in &msg.content {
                match block {
                    ContentBlock::Text(text) => {
                        for text_line in text.lines() {
                            lines.push(Line::from(Span::raw(text_line.to_string())));
                        }
                    }
                    ContentBlock::CodeBlock { language, code } => {
                        let lang_label = language.as_deref().unwrap_or("code");
                        lines.push(Line::from(Span::styled(
                            format!("  ┌─ {lang_label} ─"),
                            Style::default().fg(Color::DarkGray),
                        )));
                        for code_line in code.lines() {
                            lines.push(Line::from(Span::styled(
                                format!("  │ {code_line}"),
                                Style::default().fg(Color::Rgb(180, 180, 180)),
                            )));
                        }
                        lines.push(Line::from(Span::styled(
                            "  └────",
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                    ContentBlock::ToolUse(tool_call) => {
                        let marker = if self.show_tool_calls { "▼" } else { "▶" };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {marker} [Tool: {}]", tool_call.name),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]));
                        if self.show_tool_calls {
                            for arg_line in tool_call.arguments.lines().take(20) {
                                lines.push(Line::from(Span::styled(
                                    format!("    {arg_line}"),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                    ContentBlock::ToolResult(result) => {
                        if self.show_tool_calls {
                            let status = if result.success { "ok" } else { "err" };
                            let status_color = if result.success {
                                Color::Green
                            } else {
                                Color::Red
                            };
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled(
                                    format!("[{status}] "),
                                    Style::default().fg(status_color),
                                ),
                            ]));
                            for out_line in result.output.lines().take(10) {
                                lines.push(Line::from(Span::styled(
                                    format!("    {out_line}"),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                    ContentBlock::Thinking(text) => {
                        if self.show_tool_calls && !text.is_empty() {
                            lines.push(Line::from(Span::styled(
                                "  [Thinking...]",
                                Style::default()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::ITALIC),
                            )));
                            for thought_line in text.lines().take(5) {
                                lines.push(Line::from(Span::styled(
                                    format!("    {thought_line}"),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                    ContentBlock::Error(text) => {
                        lines.push(Line::from(Span::styled(
                            format!("  Error: {text}"),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }

            // Blank line between messages
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
