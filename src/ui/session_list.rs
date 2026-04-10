use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::Session;
use crate::ui::{border_style, highlight_style, palette};

pub struct SessionListComponent {
    pub state: ListState,
}

impl Default for SessionListComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionListComponent {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self { state }
    }

    pub fn render(&mut self, sessions: &[&Session], focused: bool, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = sessions
            .iter()
            .map(|s| {
                let time = format_relative_time(s.started_at);
                let provider = s.provider.as_str();
                let project = s
                    .project_name
                    .as_deref()
                    .unwrap_or("(unknown project)");
                let branch = s.git_branch.as_deref().unwrap_or("");
                let summary = match s.summary.as_deref() {
                    Some(text) if text.chars().count() > 80 => {
                        let mut s: String = text.chars().take(77).collect();
                        s.push_str("...");
                        s
                    }
                    Some(text) => text.to_string(),
                    None => String::new(),
                };

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(
                            provider,
                            Style::default()
                                .fg(provider_color(s.provider))
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("  ", Style::default()),
                        Span::styled(time, Style::default().fg(palette::TEXT_DIM)),
                    ]),
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(project, Style::default().fg(palette::TEXT)),
                    ]),
                ];

                if !branch.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("\u{e0a0} {branch}"),
                            Style::default().fg(palette::MAUVE),
                        ),
                    ]));
                }

                if !summary.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(summary, Style::default().fg(palette::TEXT_DIM)),
                    ]));
                }

                lines.push(Line::raw(""));

                ListItem::new(lines)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Sessions ({}) ", sessions.len()))
                    .title_style(Style::default().fg(palette::TEXT).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(border_style(focused)),
            )
            .highlight_style(highlight_style())
            .highlight_symbol("  ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }
}

fn provider_color(provider: crate::model::Provider) -> ratatui::style::Color {
    match provider {
        crate::model::Provider::ClaudeCode => palette::CLAUDE,
        crate::model::Provider::CopilotCli => palette::COPILOT,
        crate::model::Provider::GeminiCli => palette::GEMINI,
        crate::model::Provider::CodexCli => palette::CODEX,
        crate::model::Provider::OpenCode => palette::OPENCODE,
    }
}

fn format_relative_time(dt: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{}d ago", duration.num_days())
    } else {
        dt.format("%b %d").to_string()
    }
}
