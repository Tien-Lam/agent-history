use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;

use crate::cli_error::ErrorEnvelope;
use crate::config;
use crate::federated::LOCAL_SOURCE;
use crate::model::{
    split_source_prefix, CitationRef, Provider, QualifiedCitationRef, Session, SessionRef,
};

#[derive(Clone, Copy)]
pub enum SelectorShape {
    SessionRefOnly,
    SessionRefOrIdPrefix,
}

#[derive(Debug)]
pub struct SelectedSession<'a> {
    pub session: &'a Session,
    pub source: &'a str,
    pub session_ref: String,
}

#[derive(Debug)]
pub struct SelectedCitation<'a> {
    pub session: &'a Session,
    pub source: &'a str,
    pub citation: CitationRef,
    pub citation_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    InvalidSourceName(String),
    InvalidSessionRef {
        selector: String,
        message: String,
    },
    InvalidCitationRef {
        selector: String,
        message: String,
    },
    TurnRefForSession,
    SessionRefRequired(String),
    NotFound(String),
    Ambiguous {
        selector: String,
        count: usize,
        candidates: Vec<String>,
    },
}

impl ResolutionError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidSourceName(_)
            | Self::InvalidSessionRef { .. }
            | Self::InvalidCitationRef { .. }
            | Self::TurnRefForSession
            | Self::SessionRefRequired(_) => "usage",
            Self::NotFound(_) => "session-not-found",
            Self::Ambiguous { .. } => "ambiguous-session",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::InvalidSessionRef { .. } => Some(
                "Format: <provider-slug>/<session-id> or <source>:<provider-slug>/<session-id>.",
            ),
            Self::InvalidCitationRef { .. } => Some(
                "Format: <provider-slug>/<session-id>#<turn> or <source>:<provider-slug>/<session-id>#<turn>.",
            ),
            Self::TurnRefForSession => Some(
                "Use `aghist show <ref>` for a single turn, or remove the `#<turn>` suffix.",
            ),
            Self::SessionRefRequired(_) => {
                Some("Use <source>:<provider>/<session-id> for remote-source sessions.")
            }
            Self::NotFound(_) => {
                Some("Run `aghist --list --json` to see available sessions and sources.")
            }
            Self::Ambiguous { .. } => {
                Some("Use <source>:<provider>/<session-id> to choose one session explicitly.")
            }
            Self::InvalidSourceName(_) => None,
        }
    }

    pub fn into_error_envelope(self) -> ErrorEnvelope {
        let hint = self.hint();
        let mut envelope = ErrorEnvelope::new(self.kind(), self.to_string());
        if let Some(hint) = hint {
            envelope = envelope.with_hint(hint);
        }
        envelope
    }
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceName(message) => f.write_str(message),
            Self::InvalidSessionRef { selector, message } => {
                write!(f, "invalid session ref '{selector}': {message}")
            }
            Self::InvalidCitationRef { selector, message } => {
                write!(f, "invalid citation ref '{selector}': {message}")
            }
            Self::TurnRefForSession => {
                f.write_str("expected a session ref, not a turn-level citation ref")
            }
            Self::SessionRefRequired(selector) => {
                write!(
                    f,
                    "invalid session ref '{selector}': expected <provider>/<session-id>"
                )
            }
            Self::NotFound(selector) => write!(f, "Session not found: {selector}"),
            Self::Ambiguous {
                selector,
                count,
                candidates,
            } => write!(
                f,
                "session selector '{selector}' matched {count} sessions: {}",
                candidates.join(", ")
            ),
        }
    }
}

impl From<ResolutionError> for ErrorEnvelope {
    fn from(value: ResolutionError) -> Self {
        value.into_error_envelope()
    }
}

pub struct SessionResolver<'a, S = std::collections::hash_map::RandomState> {
    sessions: &'a [Session],
    source_by_session: &'a HashMap<String, String, S>,
}

impl<'a, S: BuildHasher> SessionResolver<'a, S> {
    pub fn new(sessions: &'a [Session], source_by_session: &'a HashMap<String, String, S>) -> Self {
        Self {
            sessions,
            source_by_session,
        }
    }

    pub fn source_for_session(&self, session: &'a Session) -> &'a str {
        source_for_session(self.source_by_session, session)
    }

    pub fn qualified_session_ref(&self, session: &Session) -> String {
        qualified_session_ref(self.source_by_session, session)
    }

    pub fn qualified_citation_ref(&self, session: &Session, turn: u32) -> String {
        qualified_citation_ref(self.source_by_session, session, turn)
    }

    pub fn resolve_session_selector(
        &self,
        selector: &str,
        shape: SelectorShape,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if selector.contains('#') {
            return Err(ResolutionError::TurnRefForSession);
        }

        if let Some((source, raw_ref)) = split_valid_source_prefix(selector)? {
            let session_ref = parse_session_ref(raw_ref, selector)?;
            let matches: Vec<&Session> = self
                .sessions
                .iter()
                .filter(|session| {
                    session.provider == session_ref.provider
                        && session.id == session_ref.session_id
                        && self.source_for_session(session) == source
                })
                .collect();
            return self.unique_target(selector, &matches);
        }

        if selector.contains('/') {
            let session_ref = parse_session_ref(selector, selector)?;
            let matches: Vec<&Session> = self
                .sessions
                .iter()
                .filter(|session| {
                    session.provider == session_ref.provider && session.id == session_ref.session_id
                })
                .collect();
            return self.unique_target(selector, &matches);
        }

        if matches!(shape, SelectorShape::SessionRefOnly) {
            return Err(ResolutionError::SessionRefRequired(selector.to_string()));
        }

        self.find_by_id_prefix(selector, None, None)
    }

    pub fn resolve_citation_selector(
        &self,
        selector: &str,
    ) -> Result<SelectedCitation<'a>, ResolutionError> {
        let (source, citation) =
            if let Some((source, raw_ref)) = split_valid_source_prefix(selector)? {
                (Some(source), parse_citation_ref(raw_ref, selector)?)
            } else {
                (None, parse_citation_ref(selector, selector)?)
            };

        let matches: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session.provider == citation.provider
                    && session.id == citation.session_id
                    && source.is_none_or(|source| self.source_for_session(session) == source)
            })
            .collect();
        let target = self.unique_target(selector, &matches)?;
        Ok(SelectedCitation {
            session: target.session,
            source: target.source,
            citation_ref: format!("{}#{}", target.session_ref, citation.turn),
            citation,
        })
    }

    pub fn find_by_id_prefix(
        &self,
        session_id: &str,
        provider_filter: Option<Provider>,
        source_filter: Option<&str>,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if let Some(source) = source_filter {
            validate_lookup_source(source)?;
        }
        let candidates: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| provider_filter.is_none_or(|want| session.provider == want))
            .filter(|session| {
                source_filter.is_none_or(|want| self.source_for_session(session) == want)
            })
            .filter(|session| session.id.0 == session_id || session.id.0.starts_with(session_id))
            .collect();

        let exact: Vec<&Session> = candidates
            .iter()
            .copied()
            .filter(|session| session.id.0 == session_id)
            .collect();
        let matches = if exact.is_empty() { candidates } else { exact };
        self.unique_target(session_id, &matches)
    }

    pub fn find_exact(
        &self,
        provider: Provider,
        session_id: &str,
        source_filter: Option<&str>,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if let Some(source) = source_filter {
            validate_lookup_source(source)?;
        }
        let selector = if let Some(source) = source_filter {
            format!("{source}:{}/{}", provider.slug(), session_id)
        } else {
            format!("{}/{}", provider.slug(), session_id)
        };
        let matches: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session.provider == provider
                    && session.id.0 == session_id
                    && source_filter.is_none_or(|source| self.source_for_session(session) == source)
            })
            .collect();
        self.unique_target(&selector, &matches)
    }

    fn unique_target(
        &self,
        selector: &str,
        matches: &[&'a Session],
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if matches.is_empty() {
            return Err(ResolutionError::NotFound(selector.to_string()));
        }
        if matches.len() > 1 {
            return Err(self.target_ambiguous(selector, matches));
        }
        let session = matches[0];
        Ok(SelectedSession {
            session,
            source: self.source_for_session(session),
            session_ref: self.qualified_session_ref(session),
        })
    }

    fn target_ambiguous(&self, selector: &str, matches: &[&Session]) -> ResolutionError {
        let mut candidates: Vec<String> = matches
            .iter()
            .take(8)
            .map(|session| self.qualified_session_ref(session))
            .collect();
        if matches.len() > candidates.len() {
            candidates.push(format!("... and {} more", matches.len() - candidates.len()));
        }
        ResolutionError::Ambiguous {
            selector: selector.to_string(),
            count: matches.len(),
            candidates,
        }
    }
}

pub fn source_for_session<'a, S: BuildHasher>(
    source_by_session: &'a HashMap<String, String, S>,
    session: &Session,
) -> &'a str {
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
}

pub fn qualified_session_ref<S: BuildHasher>(
    source_by_session: &HashMap<String, String, S>,
    session: &Session,
) -> String {
    let session_ref = session.session_ref().to_string();
    let source = source_for_session(source_by_session, session);
    if source == LOCAL_SOURCE {
        session_ref
    } else {
        format!("{source}:{session_ref}")
    }
}

pub fn qualified_citation_ref<S: BuildHasher>(
    source_by_session: &HashMap<String, String, S>,
    session: &Session,
    turn: u32,
) -> String {
    let source = source_for_session(source_by_session, session);
    let Some(citation) = session.citation_ref(turn) else {
        let raw_ref = format!("{}/{}#{turn}", session.provider.slug(), session.id.0);
        return if source == LOCAL_SOURCE {
            raw_ref
        } else {
            format!("{source}:{raw_ref}")
        };
    };
    QualifiedCitationRef::new(
        (source != LOCAL_SOURCE).then(|| source.to_string()),
        citation,
    )
    .to_string()
}

fn split_valid_source_prefix(raw: &str) -> Result<Option<(&str, &str)>, ResolutionError> {
    let (source, rest) = split_source_prefix(raw);
    if let Some(source) = source {
        validate_lookup_source(source)?;
        Ok(Some((source, rest)))
    } else {
        Ok(None)
    }
}

fn validate_lookup_source(source: &str) -> Result<(), ResolutionError> {
    if source == LOCAL_SOURCE {
        return Ok(());
    }
    config::validate_source_name(source).map_err(ResolutionError::InvalidSourceName)
}

fn parse_session_ref(raw: &str, full_selector: &str) -> Result<SessionRef, ResolutionError> {
    raw.parse::<SessionRef>()
        .map_err(|e| ResolutionError::InvalidSessionRef {
            selector: full_selector.to_string(),
            message: e.to_string(),
        })
}

fn parse_citation_ref(raw: &str, full_selector: &str) -> Result<CitationRef, ResolutionError> {
    raw.parse::<CitationRef>()
        .map_err(|e| ResolutionError::InvalidCitationRef {
            selector: full_selector.to_string(),
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests;
