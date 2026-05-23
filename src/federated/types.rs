use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::federated::LOCAL_SOURCE;
use crate::model::{Provider, Session};

/// Outcome of [`crate::federated::discover_federated`]. `sessions` are
/// concatenated from local + every reachable remote source. `failures` records
/// sources whose discovery raised an error or whose cache directory was
/// missing, and callers surface them as diagnostics rather than aborting.
pub struct FederatedDiscovery {
    pub sessions: Vec<Session>,
    /// Maps `Session::identity_key()` to the source tag (`"local"` or a
    /// registered source name). Sessions with no entry default to `"local"` —
    /// useful for code paths that did not go through federated discovery.
    pub source_by_session: HashMap<String, String>,
    pub failures: Vec<SourceFailure>,
}

#[derive(Debug, Clone)]
pub struct SourceFailure {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceError {
    pub source: String,
    pub error: String,
}

impl SourceFailure {
    pub fn to_error(&self) -> SourceError {
        SourceError {
            source: self.source.clone(),
            error: self.message.clone(),
        }
    }

    pub fn warning_line(&self) -> String {
        format!("warning: source '{}': {}", self.source, self.message)
    }
}

pub fn source_errors(failures: &[SourceFailure]) -> Vec<SourceError> {
    failures.iter().map(SourceFailure::to_error).collect()
}

impl FederatedDiscovery {
    /// Returns the source tag for a session, defaulting to `"local"` when the
    /// session was never seen by federated discovery.
    pub fn source_of_session(&self, session: &Session) -> &str {
        self.source_by_session
            .get(session.identity_key().as_str())
            .map_or(LOCAL_SOURCE, String::as_str)
    }

    /// Keep only sessions whose provider is in `allowed`, and drop source-map
    /// entries for any removed sessions.
    pub fn retain_providers(&mut self, allowed: &HashSet<Provider>) {
        self.sessions
            .retain(|session| allowed.contains(&session.provider));
        let retained_keys: HashSet<String> =
            self.sessions.iter().map(Session::identity_key).collect();
        self.source_by_session
            .retain(|key, _source| retained_keys.contains(key));
    }
}
