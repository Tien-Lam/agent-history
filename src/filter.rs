use chrono::{DateTime, Utc};

use crate::model::Session;

/// Criteria for filtering discovered sessions.
#[derive(Debug, Default)]
pub struct SessionFilter {
    pub provider: Option<String>,
    pub project: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

impl SessionFilter {
    /// Check whether a session matches all active filter criteria.
    #[must_use]
    pub fn matches(&self, session: &Session) -> bool {
        if let Some(ref provider) = self.provider
            && !session.provider.eq_ignore_ascii_case(provider)
        {
            return false;
        }

        if let Some(ref project) = self.project {
            let project_lower = project.to_lowercase();
            if !session.project_path.to_lowercase().contains(&project_lower) {
                return false;
            }
        }

        if let Some(since) = self.since
            && session.started_at < since
        {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;

    use super::*;
    use crate::model::SessionId;

    fn make_session(provider: &str, project: &str, started_at: DateTime<Utc>) -> Session {
        Session {
            id: SessionId::new("test-id"),
            provider: provider.to_string(),
            project_path: project.to_string(),
            started_at,
            updated_at: started_at,
            summary: String::new(),
            model: None,
            message_count: 0,
            source_path: PathBuf::new(),
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let filter = SessionFilter::default();
        let session = make_session("claude", "/home/user/project", Utc::now());
        assert!(filter.matches(&session));
    }

    #[test]
    fn filter_by_provider() {
        let filter = SessionFilter {
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        let session = make_session("claude", "/home/user/project", Utc::now());
        assert!(filter.matches(&session));

        let session2 = make_session("codex", "/home/user/project", Utc::now());
        assert!(!filter.matches(&session2));
    }

    #[test]
    fn filter_by_project_substring() {
        let filter = SessionFilter {
            project: Some("agent".to_string()),
            ..Default::default()
        };
        let session = make_session("claude", "/home/user/agent-history", Utc::now());
        assert!(filter.matches(&session));

        let session2 = make_session("claude", "/home/user/other", Utc::now());
        assert!(!filter.matches(&session2));
    }

    #[test]
    fn filter_by_since() {
        let cutoff = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let filter = SessionFilter {
            since: Some(cutoff),
            ..Default::default()
        };

        let old = make_session(
            "claude",
            "/project",
            Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap(),
        );
        assert!(!filter.matches(&old));

        let new = make_session(
            "claude",
            "/project",
            Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap(),
        );
        assert!(filter.matches(&new));
    }
}
