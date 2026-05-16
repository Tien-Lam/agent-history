use std::fmt::Write as _;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::model::{ContentBlock, Message, Role, Session};
use crate::ui::{border_style, palette, role_style, syntax};

pub(super) fn message_view_block(
    session: Option<&Session>,
    scroll_offset: u16,
    focused: bool,
) -> Block<'static> {
    let title = session.map_or_else(|| " No session selected ".to_string(), session_title);
    let scroll_indicator = if scroll_offset > 0 {
        format!(" \u{2191}{scroll_offset} ")
    } else {
        String::new()
    };

    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(
            Line::from(scroll_indicator)
                .style(Style::default().fg(palette::TEXT_DIM))
                .right_aligned(),
        )
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style(focused))
}

fn session_title(session: &Session) -> String {
    let project = session.project_name.as_deref().unwrap_or("Session");
    let model = session.model.as_deref().unwrap_or("unknown");
    let mut parts = format!(" {project} \u{2022} {model}");
    if let Some(ref usage) = session.token_usage {
        let total = usage.input_tokens + usage.output_tokens;
        if total > 0 {
            let _ = write!(parts, " \u{2022} {}k tok", total / 1000);
        }
    }
    parts.push(' ');
    parts
}

pub(super) fn render_placeholder(
    frame: &mut Frame,
    area: Rect,
    block: Block<'static>,
    message: &'static str,
) {
    let placeholder = Paragraph::new(Line::from(Span::styled(
        message,
        Style::default().fg(palette::TEXT_DIM),
    )))
    .block(block);
    frame.render_widget(placeholder, area);
}

pub(super) fn is_tool_result_echo(msg: &Message) -> bool {
    msg.role == Role::User
        && msg
            .content
            .iter()
            .all(|c| matches!(c, ContentBlock::ToolResult(_)))
}

pub(super) fn push_message_header(lines: &mut Vec<Line<'static>>, msg: &Message) {
    let time_str = msg.timestamp.format("%H:%M:%S").to_string();
    let role_label = format!(" {} ", msg.role);
    lines.push(Line::from(vec![
        Span::styled(
            role_label,
            role_style(msg.role).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(time_str, Style::default().fg(palette::TEXT_FAINT)),
    ]));
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(40),
        Style::default().fg(palette::TEXT_FAINT),
    )));
}

pub(super) fn push_text(lines: &mut Vec<Line<'static>>, text: &str) {
    for text_line in text.lines() {
        lines.push(Line::from(Span::styled(
            format!(" {text_line}"),
            Style::default().fg(palette::TEXT),
        )));
    }
}

pub(super) fn push_code_block(lines: &mut Vec<Line<'static>>, language: Option<&str>, code: &str) {
    let lang_label = language.unwrap_or("code");
    lines.push(Line::from(Span::styled(
        format!(" \u{256d}\u{2500} {lang_label} \u{2500}\u{2500}\u{2500}"),
        Style::default().fg(palette::TEXT_FAINT),
    )));
    for code_line in code.lines() {
        let mut spans = vec![Span::styled(
            " \u{2502} ",
            Style::default().fg(palette::TEXT_FAINT),
        )];
        spans.extend(syntax::highlight_line(language, code_line));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        " \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        Style::default().fg(palette::TEXT_FAINT),
    )));
}

pub(super) fn push_error(lines: &mut Vec<Line<'static>>, text: &str) {
    lines.push(Line::from(Span::styled(
        format!(" \u{2718} Error: {text}"),
        Style::default()
            .fg(palette::RED)
            .add_modifier(Modifier::BOLD),
    )));
}

pub(super) fn dim_line(text: String) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(palette::TEXT_DIM)))
}
