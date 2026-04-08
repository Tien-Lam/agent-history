use crate::error::{Error, Result};
use crate::filter::SessionFilter;
use crate::model::{Message, Session};
use crate::provider::Provider;

/// Holds all available providers and dispatches queries across them.
pub struct Registry {
    providers: Vec<Box<dyn Provider>>,
}

impl Registry {
    #[must_use]
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        Self { providers }
    }

    /// Discover sessions from all available providers, applying the given filter.
    /// Results are sorted by `started_at` descending.
    pub fn discover_sessions(&self, filter: &SessionFilter) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();

        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }
            if let Some(ref p) = filter.provider
                && !provider.name().eq_ignore_ascii_case(p)
            {
                continue;
            }
            match provider.discover_sessions() {
                Ok(discovered) => {
                    sessions.extend(discovered.into_iter().filter(|s| filter.matches(s)));
                }
                Err(e) => {
                    eprintln!("warning: {} provider: {e}", provider.name());
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        if let Some(limit) = filter.limit {
            sessions.truncate(limit);
        }

        Ok(sessions)
    }

    /// Find a single session by ID prefix. Returns an error if zero or multiple sessions match.
    pub fn find_session(&self, prefix: &str) -> Result<Session> {
        let filter = SessionFilter::default();
        let sessions = self.discover_sessions(&filter)?;

        let matches: Vec<_> = sessions
            .into_iter()
            .filter(|s| s.id.matches_prefix(prefix))
            .collect();

        match matches.len() {
            0 => Err(Error::SessionNotFound(prefix.to_string())),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => Err(Error::AmbiguousPrefix {
                prefix: prefix.to_string(),
                count: n,
            }),
        }
    }

    /// Load messages for a session, delegating to the correct provider.
    pub fn load_messages(&self, session: &Session) -> Result<Vec<Message>> {
        for provider in &self.providers {
            if provider.name() == session.provider {
                return provider.load_messages(session);
            }
        }
        Err(Error::SessionNotFound(session.id.to_string()))
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(vec![
            Box::new(super::claude::ClaudeProvider::new()),
            Box::new(super::codex::CodexProvider),
            Box::new(super::copilot::CopilotProvider),
            Box::new(super::gemini::GeminiProvider),
            Box::new(super::opencode::OpenCodeProvider),
        ])
    }
}
