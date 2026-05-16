use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::ui::palette;

pub(in crate::app) fn render_help_overlay(frame: &mut ratatui::Frame, area: Rect) {
    let header = Style::default()
        .fg(palette::ACCENT)
        .add_modifier(Modifier::BOLD);
    let key = Style::default().fg(palette::PEACH);
    let desc = Style::default().fg(palette::TEXT);

    let lines = help_overlay_lines(header, key, desc);

    let help_width = 48;
    let help_height = u16::try_from(lines.len() + 2)
        .unwrap_or(38)
        .min(area.height.saturating_sub(2));
    let x = area.width.saturating_sub(help_width) / 2;
    let y = area.height.saturating_sub(help_height) / 2;

    let help_area = Rect::new(x, y, help_width, help_height);

    frame.render_widget(Clear, help_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Help ")
                .title_style(
                    Style::default()
                        .fg(palette::TEXT)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(palette::ACCENT)),
        ),
        help_area,
    );
}

const HELP_BROWSE_ROWS: &[(&str, &str)] = &[
    ("  j / Down  ", "Next session"),
    ("  k / Up    ", "Previous session"),
    ("  Enter     ", "Open session"),
    ("  g         ", "Go to top"),
    ("  G         ", "Go to bottom"),
    ("  y         ", "Show resume command"),
    ("  s         ", "Toggle star (bookmark)"),
    ("  /         ", "Search conversations"),
    ("  H         ", "Toggle hybrid (semantic) search"),
    ("  f         ", "Open filter panel"),
    ("  Tab       ", "Switch focus"),
];

const HELP_VIEW_ROWS: &[(&str, &str)] = &[
    ("  j / Down  ", "Scroll down"),
    ("  k / Up    ", "Scroll up"),
    ("  Ctrl+D    ", "Page down"),
    ("  Ctrl+U    ", "Page up"),
    ("  g / G     ", "Top / bottom"),
    ("  t         ", "Toggle tool calls"),
    ("  r         ", "Toggle raw tool output"),
    ("  e         ", "Export session"),
    ("  y         ", "Show resume command"),
    ("  Esc       ", "Back to list"),
];

const HELP_SEARCH_ROWS: &[(&str, &str)] = &[
    ("  Type      ", "Filter sessions"),
    ("  Tab       ", "Toggle hybrid engine"),
    ("  Enter     ", "Open selected"),
    ("  Esc       ", "Cancel search"),
];

const HELP_FILTER_ROWS: &[(&str, &str)] = &[
    ("  j / k     ", "Navigate items"),
    ("  Space     ", "Toggle / cycle row"),
    ("  e         ", "Edit text field"),
    ("  Ctrl+C    ", "Clear all filters"),
    ("  Esc / f   ", "Close panel"),
];

const HELP_GLOBAL_ROWS: &[(&str, &str)] = &[
    ("  ?         ", "Toggle this help"),
    ("  q         ", "Quit"),
    ("  Ctrl+C    ", "Force quit"),
];

const HELP_SECTIONS: &[(&str, &[(&str, &str)])] = &[
    ("Browse Mode", HELP_BROWSE_ROWS),
    ("View Mode", HELP_VIEW_ROWS),
    ("Search Mode", HELP_SEARCH_ROWS),
    ("Filter Panel", HELP_FILTER_ROWS),
    ("Global", HELP_GLOBAL_ROWS),
];

fn help_overlay_lines(header: Style, key: Style, desc: Style) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, (title, rows)) in HELP_SECTIONS.iter().enumerate() {
        if index > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(*title, header)));
        lines.extend(rows.iter().map(|(shortcut, label)| {
            Line::from(vec![
                Span::styled(*shortcut, key),
                Span::styled(*label, desc),
            ])
        }));
    }
    lines
}
