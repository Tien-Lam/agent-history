use ratatui::layout::{Constraint, Direction, Layout};

use crate::model::{Message, Session};

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

        // Inline display computation so the resulting `Vec<&Session>` borrows
        // only `self.sessions` — leaving `self.session_list`, `self.stars`,
        // and `self.message_cache` free to be borrowed independently below.
        let base: Vec<&Session> = if let Some(ref ids) = self.filtered_session_ids {
            ids.iter()
                .filter_map(|id| self.sessions.iter().find(|s| s.identity_key() == *id))
                .collect()
        } else {
            self.sessions.iter().collect()
        };
        let display: Vec<&Session> = if self.filter.is_active() {
            let starred_only = self.filter.starred_only;
            let stars = &self.stars;
            base.into_iter()
                .filter(|s| self.filter.matches(s))
                .filter(|s| !starred_only || stars.is_starred(s.provider, &s.id.0))
                .collect()
        } else {
            base
        };

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
            .and_then(|s| self.message_cache.get(&s.id.0))
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
        let engine_label = if self.hybrid_available {
            Some(self.last_engine)
        } else {
            None
        };
        self.status_bar.render(
            self.mode,
            self.loading,
            &self.search_query,
            self.index_progress,
            warning_count,
            self.filter.is_active(),
            self.status_message.as_deref(),
            engine_label,
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
