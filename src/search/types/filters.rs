use chrono::{DateTime, Utc};

use crate::model::{Provider, Role};

/// Server-side filters applied alongside a `search` query. Empty fields mean
/// "do not filter on this dimension". `since`/`until` are inclusive bounds on
/// the message timestamp; `project` is a case-insensitive substring match.
#[derive(Debug, Default, Clone)]
pub struct SearchFilters {
    pub provider: Option<Provider>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub project: Option<String>,
    pub role: Option<Role>,
    pub has_tool_call: bool,
}

impl SearchFilters {
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.since.is_none()
            && self.until.is_none()
            && self.project.is_none()
            && self.role.is_none()
            && !self.has_tool_call
    }
}
