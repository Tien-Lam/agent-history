use std::time::Instant;

use crate::action::Action;

use super::super::{App, AppMode};

impl App {
    pub(super) fn dispatch_search(&mut self, action: &Action) {
        match action {
            Action::SearchStart => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.search_pending_at = None;
                self.mode = AppMode::Search;
            }
            Action::SearchInput(c) => {
                self.search_query.push(*c);
                self.search_pending_at = Some(Instant::now());
            }
            Action::SearchBackspace => {
                self.search_query.pop();
                self.search_pending_at = Some(Instant::now());
            }
            Action::ToggleHybrid => {
                if self.hybrid.available {
                    self.hybrid.enabled = !self.hybrid.enabled;
                    self.status_message = Some(if self.hybrid.enabled {
                        "Hybrid search: ON".to_string()
                    } else {
                        "Hybrid search: OFF".to_string()
                    });
                    // Re-run the current query so the engine label and result
                    // ordering reflect the new mode immediately.
                    if self.search_query.is_empty() {
                        self.last_engine = if self.hybrid.enabled {
                            "hybrid"
                        } else {
                            "lexical"
                        };
                    } else {
                        self.search_pending_at = None;
                        self.execute_search();
                    }
                } else {
                    self.status_message = Some(
                        "Hybrid search unavailable — run `aghist index --accept-download`"
                            .to_string(),
                    );
                }
            }
            Action::SearchCancel => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.search_pending_at = None;
                self.session_list.state.select(if self.sessions.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.mode = AppMode::Browse;
            }
            _ => {}
        }
    }
}
