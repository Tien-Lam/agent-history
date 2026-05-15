use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppMode;
use crate::ui::palette;

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
        engine: Option<&str>,
        frame: &mut Frame,
        area: Rect,
    ) {
        let bg = Style::default().bg(palette::SURFACE);

        if mode == AppMode::Search {
            let mut spans = vec![
                Span::styled(
                    " / ",
                    Style::default()
                        .fg(palette::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(search_query, Style::default().fg(palette::TEXT)),
                Span::styled("\u{2588}", Style::default().fg(palette::ACCENT)),
            ];
            if let Some(label) = engine {
                spans.push(Span::styled(
                    "  \u{2502} ",
                    Style::default().fg(palette::TEXT_FAINT),
                ));
                spans.push(engine_span(label));
                spans.push(Span::styled(
                    "  Tab: toggle",
                    Style::default().fg(palette::TEXT_DIM),
                ));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)).style(bg), area);
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
            ],
            AppMode::Search => vec![],
            AppMode::Help => vec![("Esc", "close")],
            AppMode::Filter => vec![
                ("j/k", "navigate"),
                ("Space", "toggle"),
                ("e", "edit"),
                ("Ctrl+C", "clear"),
                ("Esc", "close"),
            ],
            AppMode::ExportMenu => {
                vec![("j/k", "navigate"), ("Enter", "export"), ("Esc", "cancel")]
            }
        };

        let mut spans: Vec<Span> = Vec::new();

        if loading {
            spans.push(Span::styled(
                " \u{25cf} Loading ",
                Style::default()
                    .fg(palette::YELLOW)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                "\u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
        }

        for (i, (key, desc)) in keys.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(
                    "  \u{2022}  ",
                    Style::default().fg(palette::TEXT_FAINT),
                ));
            }
            spans.push(Span::styled(
                format!(" {key}"),
                Style::default()
                    .fg(palette::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {desc}"),
                Style::default().fg(palette::TEXT_DIM),
            ));
        }

        if filter_active {
            spans.push(Span::styled(
                "  \u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
            spans.push(Span::styled(
                "\u{25cf} FILTERED",
                Style::default()
                    .fg(palette::YELLOW)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        if let Some(label) = engine {
            spans.push(Span::styled(
                "  \u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
            spans.push(engine_span(label));
        }

        if warning_count > 0 {
            spans.push(Span::styled(
                "  \u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
            spans.push(Span::styled(
                format!("\u{26a0} {warning_count} warning(s)"),
                Style::default().fg(palette::RED),
            ));
        }

        if let Some((done, total)) = index_progress {
            spans.push(Span::styled(
                "  \u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
            spans.push(Span::styled(
                format!("Indexing {done}/{total}"),
                Style::default().fg(palette::YELLOW),
            ));
        }

        if let Some(msg) = status_message {
            spans.push(Span::styled(
                "  \u{2502} ",
                Style::default().fg(palette::TEXT_FAINT),
            ));
            spans.push(Span::styled(
                msg.to_string(),
                Style::default().fg(palette::GREEN),
            ));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line).style(bg), area);
    }
}

/// Visual badge for the active search engine. Hybrid is highlighted (accent
/// colour, bold) so it pops; lexical stays muted since it's the default and
/// shouldn't draw the eye.
fn engine_span(label: &str) -> Span<'static> {
    let (text, style) = match label {
        "hybrid" => (
            "engine: hybrid".to_string(),
            Style::default()
                .fg(palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        other => (
            format!("engine: {other}"),
            Style::default().fg(palette::TEXT_DIM),
        ),
    };
    Span::styled(text, style)
}
