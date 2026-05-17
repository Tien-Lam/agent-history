use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppMode;
use crate::ui::palette;

pub struct StatusBarComponent;

#[derive(Clone, Copy)]
pub struct StatusBarProps<'a> {
    pub mode: AppMode,
    pub loading: bool,
    pub search_query: &'a str,
    pub index_progress: Option<(usize, usize)>,
    pub warning_count: usize,
    pub filter_active: bool,
    pub status_message: Option<&'a str>,
    pub engine: Option<&'a str>,
}

impl Default for StatusBarComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBarComponent {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, props: StatusBarProps<'_>, frame: &mut Frame, area: Rect) {
        let bg = Style::default().bg(palette::SURFACE);

        if props.mode == AppMode::Search {
            render_search_status(props.search_query, props.engine, bg, frame, area);
            return;
        }

        let mut spans: Vec<Span> = Vec::new();
        push_loading(&mut spans, props.loading);
        push_keys(&mut spans, status_keys(props.mode));
        push_filter_badge(&mut spans, props.filter_active);
        push_engine(&mut spans, props.engine);
        push_warnings(&mut spans, props.warning_count);
        push_index_progress(&mut spans, props.index_progress);
        push_status_message(&mut spans, props.status_message);

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line).style(bg), area);
    }
}

fn render_search_status(
    search_query: &str,
    engine: Option<&str>,
    bg: Style,
    frame: &mut Frame,
    area: Rect,
) {
    let mut spans = vec![
        Span::styled(
            " / ",
            Style::default()
                .fg(palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(search_query.to_string(), Style::default().fg(palette::TEXT)),
        Span::styled("\u{2588}", Style::default().fg(palette::ACCENT)),
    ];
    if let Some(label) = engine {
        push_separator(&mut spans);
        spans.push(engine_span(label));
        spans.push(Span::styled(
            "  Tab: toggle",
            Style::default().fg(palette::TEXT_DIM),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(bg), area);
}

fn status_keys(mode: AppMode) -> &'static [(&'static str, &'static str)] {
    match mode {
        AppMode::Browse => &[
            ("j/k", "navigate"),
            ("Enter", "open"),
            ("/", "search"),
            ("f", "filter"),
            ("?", "help"),
            ("q", "quit"),
        ],
        AppMode::ViewSession => &[
            ("j/k", "scroll"),
            ("t", "tool calls"),
            ("e", "export"),
            ("Esc", "back"),
            ("?", "help"),
        ],
        AppMode::Search => &[],
        AppMode::Help => &[("Esc", "close")],
        AppMode::Filter => &[
            ("j/k", "navigate"),
            ("Space", "toggle"),
            ("e", "edit"),
            ("Ctrl+C", "clear"),
            ("Esc", "close"),
        ],
        AppMode::ExportMenu => &[("j/k", "navigate"), ("Enter", "export"), ("Esc", "cancel")],
    }
}

fn push_loading(spans: &mut Vec<Span<'_>>, loading: bool) {
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
}

fn push_keys(spans: &mut Vec<Span<'_>>, keys: &[(&str, &str)]) {
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
}

fn push_filter_badge(spans: &mut Vec<Span<'_>>, filter_active: bool) {
    if filter_active {
        push_separator(spans);
        spans.push(Span::styled(
            "\u{25cf} FILTERED",
            Style::default()
                .fg(palette::YELLOW)
                .add_modifier(Modifier::BOLD),
        ));
    }
}

fn push_engine(spans: &mut Vec<Span<'_>>, engine: Option<&str>) {
    if let Some(label) = engine {
        push_separator(spans);
        spans.push(engine_span(label));
    }
}

fn push_warnings(spans: &mut Vec<Span<'_>>, warning_count: usize) {
    if warning_count > 0 {
        push_separator(spans);
        spans.push(Span::styled(
            format!("\u{26a0} {warning_count} warning(s)"),
            Style::default().fg(palette::RED),
        ));
    }
}

fn push_index_progress(spans: &mut Vec<Span<'_>>, index_progress: Option<(usize, usize)>) {
    if let Some((done, total)) = index_progress {
        push_separator(spans);
        spans.push(Span::styled(
            format!("Indexing {done}/{total}"),
            Style::default().fg(palette::YELLOW),
        ));
    }
}

fn push_status_message(spans: &mut Vec<Span<'_>>, status_message: Option<&str>) {
    if let Some(msg) = status_message {
        push_separator(spans);
        spans.push(Span::styled(
            msg.to_string(),
            Style::default().fg(palette::GREEN),
        ));
    }
}

fn push_separator(spans: &mut Vec<Span<'_>>) {
    spans.push(Span::styled(
        "  \u{2502} ",
        Style::default().fg(palette::TEXT_FAINT),
    ));
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
