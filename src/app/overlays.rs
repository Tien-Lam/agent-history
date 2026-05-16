use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::export::ExportFormat;
use crate::ui::palette;

mod filter;
mod help;

pub(super) use filter::{push_date_char, render_filter_overlay};
pub(super) use help::render_help_overlay;

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
