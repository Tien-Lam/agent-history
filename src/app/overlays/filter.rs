use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::ui::palette;

use super::super::state::FilterState;

mod lines;

use lines::filter_lines;

pub(in crate::app) fn push_date_char(date: &mut Option<chrono::NaiveDate>, c: char) {
    if !c.is_ascii_digit() && c != '-' {
        return;
    }
    let mut buf = date.map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());
    buf.push(c);
    *date = chrono::NaiveDate::parse_from_str(&buf, "%Y-%m-%d").ok();
}

pub(in crate::app) fn render_filter_overlay(
    frame: &mut ratatui::Frame,
    area: Rect,
    filter: &FilterState,
) {
    let lines = filter_lines(filter);

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
