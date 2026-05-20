use std::collections::HashMap;
use std::path::PathBuf;

use chrono::TimeZone as _;

use super::*;
use crate::federated::LOCAL_SOURCE;
use crate::model::{Message, Provider, Role, SessionId};
use crate::provider::ProviderError;

struct FailingProvider {
    base_dirs: Vec<PathBuf>,
}

impl HistoryProvider for FailingProvider {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.base_dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        Ok(Vec::new())
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        Err(ProviderError::Parse {
            path: session.source_path.clone(),
            reason: "bad fixture".to_string(),
        })
    }
}

fn test_session() -> Session {
    Session {
        id: SessionId("bad-session".to_string()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("broken".to_string()),
        git_branch: None,
        started_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from("/tmp/bad-session.jsonl"),
    }
}

#[test]
fn message_filter_records_warning_when_session_load_fails() {
    let session = test_session();
    let source_by_session = HashMap::from([(session.identity_key(), LOCAL_SOURCE.to_string())]);
    let discovery = FederatedDiscovery {
        sessions: vec![session],
        source_by_session,
        failures: Vec::new(),
    };
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(FailingProvider {
        base_dirs: Vec::new(),
    })];
    let filters = SearchFilters {
        role: Some(Role::User),
        ..SearchFilters::default()
    };

    let page = list_sessions_page(
        &providers,
        discovery,
        ListSessionsRequest {
            limit: 10,
            cursor: None,
            filters: &filters,
            metadata_keys: None,
        },
    )
    .unwrap();

    assert!(page.sessions.is_empty());
    assert_eq!(page.warnings.len(), 1);
    assert!(page.warnings[0]
        .warning_line()
        .contains("claude-code/bad-session"));
    assert!(page.warnings[0].warning_line().contains("bad fixture"));
}
