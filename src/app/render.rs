use ratatui::layout::{Constraint, Direction, Layout};

use crate::model::{Message, Session};
use crate::ui::status_bar::StatusBarProps;

use super::overlays::{render_export_overlay, render_filter_overlay, render_help_overlay};
use super::{App, AppMode};

impl App {
    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // main content
                Constraint::Length(1), // status bar
            ])
            .split(size);

        let content_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // session list
                Constraint::Percentage(70), // conversation detail
            ])
            .split(main_layout[0]);

        let display_indices = self.display_session_indices();
        let display: Vec<&Session> = display_indices
            .into_iter()
            .map(|idx| &self.sessions[idx])
            .collect();

        // Session list
        let list_focused = self.mode == AppMode::Browse || self.mode == AppMode::Search;
        let stars = &self.stars;
        let is_starred = |s: &Session| stars.is_starred(s.provider, &s.id.0);
        self.session_list.render(
            &display,
            list_focused,
            &is_starred,
            frame,
            content_layout[0],
        );

        // Message view
        let selected_idx = self.session_list.selected_index();
        let selected_session = selected_idx.and_then(|i| display.get(i).copied());
        let messages = selected_session
            .and_then(|s| self.message_cache.get(&s.identity_key()))
            .map(|m: &Vec<Message>| m.as_slice());

        let view_focused = self.mode == AppMode::ViewSession;
        self.message_view.render(
            selected_session,
            messages,
            view_focused,
            frame,
            content_layout[1],
        );

        // Status bar
        let warning_count = self.warnings.len();
        // Show the engine indicator only when the embedding pipeline is wired
        // up — otherwise lexical is the only option and the badge would just
        // be visual noise.
        let engine_label = if self.hybrid.available {
            Some(self.last_engine)
        } else {
            None
        };
        self.status_bar.render(
            StatusBarProps {
                mode: self.mode,
                loading: self.lifecycle.loading,
                search_query: &self.search_query,
                index_progress: self.index_progress,
                warning_count,
                filter_active: self.filter.is_active(),
                status_message: self.status_message.as_deref(),
                engine: engine_label,
            },
            frame,
            main_layout[1],
        );

        // Help overlay
        if self.mode == AppMode::Help {
            render_help_overlay(frame, size);
        }

        // Filter overlay
        if self.mode == AppMode::Filter {
            render_filter_overlay(frame, size, &self.filter);
        }

        // Export overlay
        if self.mode == AppMode::ExportMenu {
            render_export_overlay(frame, size, self.export_cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::{TimeZone, Utc};
    use ratatui::{backend::TestBackend, Terminal};

    use crate::config::Config;
    use crate::model::{Provider, Role, Session, SessionId};
    use crate::stars::StarStore;

    use super::*;

    fn session(id: &str, summary: &str) -> Session {
        Session {
            id: SessionId(id.to_string()),
            provider: Provider::ClaudeCode,
            project_path: None,
            project_name: Some("filter-render".to_string()),
            git_branch: None,
            started_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: Some(summary.to_string()),
            model: None,
            token_usage: None,
            message_count: 1,
            source_path: std::path::PathBuf::from(format!("/tmp/{id}.jsonl")),
        }
    }

    fn render_to_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let mut result = String::new();
        for y in area.y..area.y + area.height {
            let mut line = String::new();
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    line.push_str(cell.symbol());
                }
            }
            result.push_str(line.trim_end());
            result.push('\n');
        }
        result
    }

    #[test]
    fn render_applies_message_level_filter_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::with_stars(
            Vec::new(),
            Config::default(),
            StarStore::load_from(&tmp.path().join("metadata.db")),
        );
        let kept = session("tool-session", "Tool session");
        let filtered = session("plain-session", "Plain session");
        app.msg_filter_session_ids = Some(HashSet::from([kept.identity_key()]));
        app.filter.role = Some(Role::Tool);
        app.sessions = vec![kept, filtered];
        app.lifecycle.loading = false;
        app.session_list.state.select(Some(0));

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();

        let text = render_to_text(&terminal);
        assert!(
            text.contains("Tool session"),
            "message-level filter should keep the matching session, got:\n{text}"
        );
        assert!(
            !text.contains("Plain session"),
            "message-level filter should hide non-matching sessions, got:\n{text}"
        );
    }
}
