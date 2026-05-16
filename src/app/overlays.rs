use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::state::{FilterField, FilterState};
use crate::export::ExportFormat;
use crate::model::Provider;
use crate::ui::palette;

mod help;

pub(super) use help::render_help_overlay;

pub(super) fn push_date_char(date: &mut Option<chrono::NaiveDate>, c: char) {
    if !c.is_ascii_digit() && c != '-' {
        return;
    }
    let mut buf = date.map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());
    buf.push(c);
    *date = chrono::NaiveDate::parse_from_str(&buf, "%Y-%m-%d").ok();
}

pub(super) fn render_filter_overlay(frame: &mut ratatui::Frame, area: Rect, filter: &FilterState) {
    let providers = Provider::all();
    let mut lines: Vec<Line<'static>> = Vec::new();

    let header = Style::default()
        .fg(palette::ACCENT)
        .add_modifier(Modifier::BOLD);
    let selected_style = Style::default().bg(palette::OVERLAY);

    push_provider_filter_lines(&mut lines, providers, filter, header, selected_style);
    push_value_filter_lines(&mut lines, providers.len(), filter, header, selected_style);
    push_toggle_filter_lines(&mut lines, filter, selected_style);

    let panel_width = 40;
    let panel_height = u16::try_from(lines.len() + 2)
        .unwrap_or(20)
        .min(area.height.saturating_sub(2));
    let x = area.width.saturating_sub(panel_width) / 2;
    let y = area.height.saturating_sub(panel_height) / 2;

    let panel_area = Rect::new(x, y, panel_width, panel_height);

    frame.render_widget(Clear, panel_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Filter ")
                .title_style(
                    Style::default()
                        .fg(palette::TEXT)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(palette::YELLOW)),
        ),
        panel_area,
    );
}

fn push_provider_filter_lines(
    lines: &mut Vec<Line<'static>>,
    providers: &[Provider],
    filter: &FilterState,
    header: Style,
    selected_style: Style,
) {
    lines.push(Line::from(Span::styled("Providers", header)));
    for (i, p) in providers.iter().enumerate() {
        let enabled = filter.provider_enabled.get(p).copied().unwrap_or(true);
        let checkbox = if enabled { "\u{25c9}" } else { "\u{25ef}" };
        let mut line = Line::from(vec![
            Span::styled(
                format!("  {checkbox} "),
                Style::default().fg(if enabled {
                    palette::GREEN
                } else {
                    palette::TEXT_FAINT
                }),
            ),
            Span::styled(p.as_str(), Style::default().fg(palette::TEXT)),
        ]);
        if filter.cursor == i {
            line = line.style(selected_style);
        }
        lines.push(line);
    }
}

fn push_value_filter_lines(
    lines: &mut Vec<Line<'static>>,
    provider_count: usize,
    filter: &FilterState,
    header: Style,
    selected_style: Style,
) {
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("Filters", header)));

    let proj_idx = provider_count;
    let proj_value = if filter.project_query.is_empty() {
        "(any)".to_string()
    } else {
        filter.project_query.clone()
    };
    let editing_proj = filter.editing_field == Some(FilterField::Project);
    let proj_suffix = if editing_proj { "\u{2588}" } else { "" };
    let mut proj_line = Line::from(vec![
        Span::styled("  Project: ", Style::default().fg(palette::PEACH)),
        Span::styled(
            format!("{proj_value}{proj_suffix}"),
            Style::default().fg(palette::TEXT),
        ),
    ]);
    if filter.cursor == proj_idx {
        proj_line = proj_line.style(selected_style);
    }
    lines.push(proj_line);

    let from_idx = proj_idx + 1;
    let from_value = filter
        .date_from
        .map_or_else(|| "(any)".to_string(), |d| d.format("%Y-%m-%d").to_string());
    let editing_from = filter.editing_field == Some(FilterField::DateFrom);
    let from_suffix = if editing_from { "\u{2588}" } else { "" };
    let mut from_line = Line::from(vec![
        Span::styled("  From:    ", Style::default().fg(palette::PEACH)),
        Span::styled(
            format!("{from_value}{from_suffix}"),
            Style::default().fg(palette::TEXT),
        ),
    ]);
    if filter.cursor == from_idx {
        from_line = from_line.style(selected_style);
    }
    lines.push(from_line);

    let to_idx = from_idx + 1;
    let to_value = filter
        .date_to
        .map_or_else(|| "(any)".to_string(), |d| d.format("%Y-%m-%d").to_string());
    let editing_to = filter.editing_field == Some(FilterField::DateTo);
    let to_suffix = if editing_to { "\u{2588}" } else { "" };
    let mut to_line = Line::from(vec![
        Span::styled("  To:      ", Style::default().fg(palette::PEACH)),
        Span::styled(
            format!("{to_value}{to_suffix}"),
            Style::default().fg(palette::TEXT),
        ),
    ]);
    if filter.cursor == to_idx {
        to_line = to_line.style(selected_style);
    }
    lines.push(to_line);
}

fn push_toggle_filter_lines(
    lines: &mut Vec<Line<'static>>,
    filter: &FilterState,
    selected_style: Style,
) {
    let role_idx = FilterState::role_idx();
    let role_value = filter
        .role
        .map_or_else(|| "(any)".to_string(), |r| r.slug().to_string());
    let mut role_line = Line::from(vec![
        Span::styled("  Role:    ", Style::default().fg(palette::PEACH)),
        Span::styled(role_value, Style::default().fg(palette::TEXT)),
    ]);
    if filter.cursor == role_idx {
        role_line = role_line.style(selected_style);
    }
    lines.push(role_line);

    let tool_idx = FilterState::tool_call_idx();
    let tool_marker = if filter.has_tool_call {
        "\u{25c9}"
    } else {
        "\u{25ef}"
    };
    let mut tool_line = Line::from(vec![
        Span::styled(
            format!("  {tool_marker} "),
            Style::default().fg(if filter.has_tool_call {
                palette::GREEN
            } else {
                palette::TEXT_FAINT
            }),
        ),
        Span::styled("Has tool call", Style::default().fg(palette::TEXT)),
    ]);
    if filter.cursor == tool_idx {
        tool_line = tool_line.style(selected_style);
    }
    lines.push(tool_line);

    let starred_idx = FilterState::starred_idx();
    let starred_marker = if filter.starred_only {
        "\u{25c9}"
    } else {
        "\u{25ef}"
    };
    let mut starred_line = Line::from(vec![
        Span::styled(
            format!("  {starred_marker} "),
            Style::default().fg(if filter.starred_only {
                palette::YELLOW
            } else {
                palette::TEXT_FAINT
            }),
        ),
        Span::styled("Starred only", Style::default().fg(palette::TEXT)),
    ]);
    if filter.cursor == starred_idx {
        starred_line = starred_line.style(selected_style);
    }
    lines.push(starred_line);
}

pub(super) fn render_export_overlay(frame: &mut ratatui::Frame, area: Rect, cursor: usize) {
    let formats = ExportFormat::all();
    let selected_style = Style::default().bg(palette::OVERLAY);

    let mut lines: Vec<Line> = Vec::new();
    for (i, fmt) in formats.iter().enumerate() {
        let marker = if i == cursor { "\u{25b8}" } else { " " };
        let mut line = Line::from(vec![
            Span::styled(
                format!(" {marker} "),
                Style::default()
                    .fg(palette::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} (.{})", fmt.label(), fmt.extension()),
                Style::default().fg(palette::TEXT),
            ),
        ]);
        if i == cursor {
            line = line.style(selected_style);
        }
        lines.push(line);
    }

    let panel_width = 28;
    let panel_height = u16::try_from(lines.len() + 2)
        .unwrap_or(6)
        .min(area.height.saturating_sub(2));
    let x = area.width.saturating_sub(panel_width) / 2;
    let y = area.height.saturating_sub(panel_height) / 2;

    let panel_area = Rect::new(x, y, panel_width, panel_height);

    frame.render_widget(Clear, panel_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Export As ")
                .title_style(
                    Style::default()
                        .fg(palette::TEXT)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(palette::GREEN)),
        ),
        panel_area,
    );
}
