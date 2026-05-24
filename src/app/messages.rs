use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};

use crate::action::Action;
use crate::model::{Provider, Session, SessionId};

use super::App;

impl App {
    pub(super) fn load_messages_cached(
        &mut self,
        session_id: &str,
        source_path: &Path,
        provider_type: Provider,
    ) {
        if self.message_cache.contains(session_id) {
            tracing::debug!(session_id, "message cache hit");
            return;
        }

        tracing::debug!(
            session_id,
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
                    self.message_cache.put(session_id.to_string(), messages);
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
        if let Some((session_id, source_path, provider)) = self.resolve_selected_session() {
            self.load_messages_cached(&session_id, &source_path, provider);
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
