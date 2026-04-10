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

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn render(
        &self,
        mode: AppMode,
        loading: bool,
        search_query: &str,
        index_progress: Option<(usize, usize)>,
        warning_count: usize,
        filter_active: bool,
        status_message: Option<&str>,
        frame: &mut Frame,
        area: Rect,
    ) {
        let bg = Style::default().bg(Color::Rgb(30, 30, 30));

        if mode == AppMode::Search {
            let line = Line::from(vec![
                Span::styled(
                    " / ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(search_query),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ]);
            frame.render_widget(Paragraph::new(line).style(bg), area);
            return;
        }

        let keys = match mode {
            AppMode::Browse => vec![
                ("j/k", "navigate"),
                ("Enter", "open"),
                ("/", "search"),
                ("f", "filter"),
                ("?", "help"),
                ("q", "quit"),
            ],
            AppMode::ViewSession => vec![
                ("j/k", "scroll"),
                ("t", "tool calls"),
                ("e", "export"),
                ("Esc", "back"),
                ("?", "help"),
                ("q", "quit"),
            ],
            AppMode::Search => unreachable!(),
            AppMode::Help => vec![("Esc", "close")],
            AppMode::Filter => vec![
                ("j/k", "navigate"),
                ("Space", "toggle"),
                ("e", "edit"),
                ("Ctrl+C", "clear all"),
                ("Esc", "close"),
            ],
            AppMode::ExportMenu => vec![
                ("j/k", "navigate"),
                ("Enter", "export"),
                ("Esc", "cancel"),
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
            spans.push(Span::raw("│ "));
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

        if filter_active {
            spans.push(Span::raw("  │ "));
            spans.push(Span::styled(
                "FILTERED",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        if warning_count > 0 {
            spans.push(Span::raw("  │ "));
            spans.push(Span::styled(
                format!("{warning_count} warning(s)"),
                Style::default().fg(Color::Red),
            ));
        }

        if let Some((done, total)) = index_progress {
            spans.push(Span::raw("  │ "));
            spans.push(Span::styled(
                format!("Indexing {done}/{total}"),
                Style::default().fg(Color::Yellow),
            ));
        }

        if let Some(msg) = status_message {
            spans.push(Span::raw("  │ "));
            spans.push(Span::styled(
                msg.to_string(),
                Style::default().fg(Color::Green),
            ));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line).style(bg), area);
    }
}
