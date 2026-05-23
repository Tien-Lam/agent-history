use crate::model::{Provider, Session};

use super::App;

mod filter;

pub(in crate::app) use filter::{cycle_role, FilterField, FilterState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browse,
    ViewSession,
    Search,
    Help,
    Filter,
    ExportMenu,
}

impl App {
    pub(super) fn display_sessions(&self) -> Vec<&Session> {
        let base: Vec<&Session> = if let Some(ref ids) = self.filtered_session_ids {
            ids.iter()
                .filter_map(|id| self.sessions.iter().find(|s| s.identity_key() == *id))
                .collect()
        } else {
            self.sessions.iter().collect()
        };

        let starred_only = self.filter.starred_only;
        let msg_ids = self.msg_filter_session_ids.as_ref();
        if self.filter.is_active() {
            base.into_iter()
                .filter(|s| self.filter.matches(s))
                .filter(|s| !starred_only || self.stars.is_starred(s.provider, &s.id.0))
                .filter(|s| msg_ids.is_none_or(|ids| ids.contains(&s.identity_key())))
                .collect()
        } else {
            base
        }
    }

    pub(super) fn display_count(&self) -> usize {
        self.display_sessions().len()
    }

    pub(super) fn resolve_selected_session(
        &self,
    ) -> Option<(String, std::path::PathBuf, Provider)> {
        let idx = self.session_list.selected_index()?;
        let display = self.display_sessions();
        display
            .get(idx)
            .map(|s| (s.id.0.clone(), s.source_path.clone(), s.provider))
    }

    /// Ensure the selection index sits within the displayed-session range.
    /// Called after operations that may shrink the visible list (e.g.
    /// unstarring while the starred-only filter is active).
    pub(super) fn clamp_selection(&mut self) {
        let count = self.display_count();
        let new_sel = match self.session_list.selected_index() {
            _ if count == 0 => None,
            Some(i) if i >= count => Some(count - 1),
            Some(i) => Some(i),
            None => Some(0),
        };
        self.session_list.state.select(new_sel);
        self.preload_focused_session();
    }

    pub(super) fn apply_filters(&mut self) {
        let count = self.display_count();
        self.session_list
            .state
            .select(if count > 0 { Some(0) } else { None });
        self.preload_focused_session();
    }
}
