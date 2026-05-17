use crate::model::{Provider, Role, Session};

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browse,
    ViewSession,
    Search,
    Help,
    Filter,
    ExportMenu,
}

#[derive(Debug, Clone)]
pub struct FilterState {
    pub provider_enabled: std::collections::HashMap<Provider, bool>,
    pub project_query: String,
    pub date_from: Option<chrono::NaiveDate>,
    pub date_to: Option<chrono::NaiveDate>,
    /// `None` = any role; cycles None -> User -> Assistant -> Tool -> None.
    /// Filters the session list to sessions containing at least one message
    /// with this role (resolved via the search index).
    pub role: Option<Role>,
    /// When true, restrict to sessions that have at least one tool-call
    /// message (resolved via the search index).
    pub has_tool_call: bool,
    pub starred_only: bool,
    pub cursor: usize,
    pub editing_field: Option<FilterField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Project,
    DateFrom,
    DateTo,
}

impl FilterState {
    pub(super) fn new() -> Self {
        let mut provider_enabled = std::collections::HashMap::new();
        for p in Provider::all() {
            provider_enabled.insert(*p, true);
        }
        Self {
            provider_enabled,
            project_query: String::new(),
            date_from: None,
            date_to: None,
            role: None,
            has_tool_call: false,
            starred_only: false,
            cursor: 0,
            editing_field: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.provider_enabled.values().any(|v| !v)
            || !self.project_query.is_empty()
            || self.date_from.is_some()
            || self.date_to.is_some()
            || self.role.is_some()
            || self.has_tool_call
            || self.starred_only
    }

    /// True when at least one message-level filter is active. Message-level
    /// filters (role, has-tool-call) require the search index to resolve to
    /// session IDs and are applied as a separate set-intersection step.
    pub fn has_message_filter(&self) -> bool {
        self.role.is_some() || self.has_tool_call
    }

    pub(super) fn matches(&self, session: &Session) -> bool {
        if !self
            .provider_enabled
            .get(&session.provider)
            .copied()
            .unwrap_or(true)
        {
            return false;
        }

        if !self.project_query.is_empty() {
            let query = self.project_query.to_lowercase();
            let name_match = session
                .project_name
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(&query));
            let path_match = session
                .project_path
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|p| p.to_lowercase().contains(&query));
            if !name_match && !path_match {
                return false;
            }
        }

        if let Some(from) = self.date_from {
            if session.started_at.date_naive() < from {
                return false;
            }
        }
        if let Some(to) = self.date_to {
            if session.started_at.date_naive() > to {
                return false;
            }
        }

        true
    }

    pub(super) fn item_count() -> usize {
        // providers + project + date_from + date_to + role + has_tool_call + starred_only
        Provider::all().len() + 6
    }

    pub(super) fn role_idx() -> usize {
        Provider::all().len() + 3
    }

    pub(super) fn tool_call_idx() -> usize {
        Provider::all().len() + 4
    }

    pub(super) fn starred_idx() -> usize {
        Provider::all().len() + 5
    }
}

/// Cycle through the role filter: None -> User -> Assistant -> Tool -> None.
/// Skips `System` because session-list filtering treats system messages as
/// noise (not user-facing turns).
pub(super) fn cycle_role(role: Option<Role>) -> Option<Role> {
    match role {
        None => Some(Role::User),
        Some(Role::User) => Some(Role::Assistant),
        Some(Role::Assistant) => Some(Role::Tool),
        Some(Role::Tool | Role::System) => None,
    }
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
