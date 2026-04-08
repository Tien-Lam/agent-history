use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::Session;

pub struct SessionListComponent {
    pub state: ListState,
}

impl SessionListComponent {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self { state }
    }

    pub fn render(&mut self, sessions: &[Session], focused: bool, frame: &mut Frame, area: Rect) {
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
                let summary = s
                    .summary
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect::<String>();

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(time, Style::default().fg(Color::DarkGray)),
                        Span::raw("  "),
                        Span::styled(
                            provider,
                            Style::default()
                                .fg(provider_color(s.provider))
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(project, Style::default().fg(Color::White)),
                    ]),
                ];

                if !branch.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(branch, Style::default().fg(Color::Magenta)),
                    ]));
                }

                if !summary.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(summary, Style::default().fg(Color::DarkGray)),
                    ]));
                }

                // Empty line as separator
                lines.push(Line::raw(""));

                ListItem::new(lines)
            })
            .collect();

        let border_style = if focused {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Sessions ({}) ", sessions.len()))
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }
}

fn provider_color(provider: crate::model::Provider) -> Color {
    match provider {
        crate::model::Provider::ClaudeCode => Color::Rgb(204, 120, 50),
        crate::model::Provider::CopilotCli => Color::Rgb(100, 200, 100),
        crate::model::Provider::GeminiCli => Color::Rgb(66, 133, 244),
        crate::model::Provider::CodexCli => Color::Rgb(200, 200, 200),
        crate::model::Provider::OpenCode => Color::Rgb(150, 100, 200),
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
