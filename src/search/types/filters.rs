use chrono::{DateTime, Utc};

use crate::model::{ContentBlock, Message, Provider, Role, Session};

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

    pub fn project_needle(&self) -> Option<String> {
        self.project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty())
    }

    pub fn needs_message_scan(&self) -> bool {
        self.role.is_some() || self.has_tool_call
    }

    pub fn matches_session(&self, session: &Session) -> bool {
        self.matches_session_with_project_needle(session, self.project_needle().as_deref())
    }

    pub fn matches_session_with_project_needle(
        &self,
        session: &Session,
        project_needle: Option<&str>,
    ) -> bool {
        if let Some(want) = self.provider {
            if session.provider != want {
                return false;
            }
        }
        if let Some(since) = self.since {
            if session.started_at < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if session.started_at > until {
                return false;
            }
        }
        if let Some(needle) = project_needle {
            let project = session
                .project_name
                .as_deref()
                .map(str::to_lowercase)
                .unwrap_or_default();
            if !project.contains(needle) {
                return false;
            }
        }
        true
    }

    pub fn matches_message(&self, message: &Message) -> bool {
        if let Some(role) = self.role {
            if message.role != role {
                return false;
            }
        }
        if self.has_tool_call
            && !message
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse(_)))
        {
            return false;
        }
        true
    }
}
