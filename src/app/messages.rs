use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};

use crate::action::Action;
use crate::model::{Provider, Session, SessionId};

use super::App;

impl App {
    pub(super) fn load_messages_cached(
        &mut self,
        cache_key: &str,
        session_id: &str,
        source_path: &Path,
        provider_type: Provider,
    ) {
        if self.message_cache.contains(cache_key) {
            tracing::debug!(session_id, cache_key, "message cache hit");
            return;
        }

        tracing::debug!(
            session_id,
            cache_key,
            source_path = %source_path.display(),
            provider = ?provider_type,
            "loading messages (cache miss)"
        );

        let tmp_session = Session {
            id: SessionId(session_id.to_string()),
            provider: provider_type,
            project_path: None,
            project_name: None,
            git_branch: None,
            started_at: temp_session_started_at(source_path),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 0,
            source_path: source_path.to_path_buf(),
        };

        let provider = self
            .providers
            .iter()
            .find(|p| p.provider() == provider_type);

        if let Some(provider) = provider {
            match provider.load_messages(&tmp_session) {
                Ok(mut messages) => {
                    tracing::info!(
                        session_id,
                        provider = ?provider_type,
                        message_count = messages.len(),
                        "messages loaded successfully"
                    );
                    let max = self.config.max_messages_per_session;
                    if messages.len() > max {
                        let total = messages.len();
                        messages.truncate(max);
                        self.warnings.push(format!(
                            "Session truncated: showing {max} of {total} messages"
                        ));
                    }
                    if messages.is_empty() {
                        tracing::warn!(
                            session_id,
                            source_path = %source_path.display(),
                            "provider returned 0 messages — possible format mismatch"
                        );
                    }
                    self.message_cache.put(cache_key.to_string(), messages);
                }
                Err(e) => {
                    tracing::error!(
                        session_id,
                        source_path = %source_path.display(),
                        error = %e,
                        "failed to load messages"
                    );
                    let _ = self
                        .action_tx
                        .send(Action::LoadError(format!("Failed to load messages: {e}")));
                }
            }
        } else {
            tracing::error!(
                session_id,
                provider = ?provider_type,
                "no matching provider found for session"
            );
        }
    }

    /// Load messages for whichever session is currently focused in the list,
    /// so the message panel always shows content alongside the session list.
    pub(super) fn preload_focused_session(&mut self) {
        if let Some((cache_key, session_id, source_path, provider)) =
            self.resolve_selected_session()
        {
            self.load_messages_cached(&cache_key, &session_id, &source_path, provider);
            self.message_view.reset_scroll();
        }
    }
}

fn temp_session_started_at(source_path: &Path) -> DateTime<Utc> {
    source_path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .map_or_else(
            |_| {
                Utc.timestamp_opt(0, 0)
                    .single()
                    .expect("unix epoch timestamp is valid")
            },
            DateTime::<Utc>::from,
        )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{TimeZone, Utc};

    use crate::config::Config;
    use crate::model::{ContentBlock, Message, MessageId, Role};
    use crate::provider::{HistoryProvider, ProviderError};
    use crate::stars::StarStore;

    use super::*;

    struct PathEchoProvider {
        base_dirs: Vec<PathBuf>,
    }

    impl HistoryProvider for PathEchoProvider {
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
            Ok(vec![Message {
                id: MessageId("m1".to_string()),
                role: Role::User,
                timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                content: vec![ContentBlock::Text(
                    session.source_path.display().to_string(),
                )],
                model: None,
                token_usage: None,
            }])
        }
    }

    fn session_with_path(id: &str, source_path: PathBuf) -> Session {
        Session {
            id: SessionId(id.to_string()),
            provider: Provider::ClaudeCode,
            project_path: None,
            project_name: None,
            git_branch: None,
            started_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 1,
            source_path,
        }
    }

    #[test]
    fn message_cache_distinguishes_duplicate_session_ids_by_identity_key() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::with_stars(
            vec![Box::new(PathEchoProvider {
                base_dirs: Vec::new(),
            })],
            Config::default(),
            StarStore::ephemeral(),
        );
        let first = session_with_path("same-id", tmp.path().join("first.jsonl"));
        let second = session_with_path("same-id", tmp.path().join("second.jsonl"));

        app.load_messages_cached(
            &first.identity_key(),
            &first.id.0,
            &first.source_path,
            first.provider,
        );
        app.load_messages_cached(
            &second.identity_key(),
            &second.id.0,
            &second.source_path,
            second.provider,
        );

        assert_eq!(app.message_cache.len(), 2);
        let first_text = app.message_cache.get(&first.identity_key()).unwrap()[0].content[0]
            .as_text()
            .unwrap()
            .to_string();
        let second_text = app.message_cache.get(&second.identity_key()).unwrap()[0].content[0]
            .as_text()
            .unwrap()
            .to_string();
        assert_ne!(first_text, second_text);
    }

    trait TestContentText {
        fn as_text(&self) -> Option<&str>;
    }

    impl TestContentText for ContentBlock {
        fn as_text(&self) -> Option<&str> {
            match self {
                ContentBlock::Text(text) => Some(text),
                _ => None,
            }
        }
    }
}
