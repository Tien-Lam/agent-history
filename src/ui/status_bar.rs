use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppMode;

pub struct StatusBarComponent;

impl Default for StatusBarComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBarComponent {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, mode: AppMode, loading: bool, frame: &mut Frame, area: Rect) {
        let keys = match mode {
            AppMode::Browse => vec![
                ("j/k", "navigate"),
                ("Enter", "open"),
                ("/", "search"),
                ("?", "help"),
                ("q", "quit"),
            ],
            AppMode::ViewSession => vec![
                ("j/k", "scroll"),
                ("t", "tool calls"),
                ("Esc", "back"),
                ("?", "help"),
                ("q", "quit"),
            ],
            AppMode::Search => vec![
                ("Enter", "search"),
                ("Esc", "cancel"),
            ],
            AppMode::Help => vec![
                ("Esc", "close"),
            ],
        };

        let mut spans: Vec<Span> = Vec::new();

        if loading {
            spans.push(Span::styled(
                " Loading... ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" │ "));
        }

        for (i, (key, desc)) in keys.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                *key,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(": {desc}"),
                Style::default().fg(Color::DarkGray),
            ));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 30)));
        frame.render_widget(paragraph, area);
    }
}
