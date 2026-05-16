//! Stable citation references for individual messages within a session.
//!
//! A [`SessionRef`] identifies a session via `(provider, session_id)`,
//! formatted as `<provider-slug>/<session-id>`.
//!
//! A [`CitationRef`] identifies one message via the triple `(provider,
//! session_id, turn)`, formatted as `<provider-slug>/<session-id>#<turn>`
//! (e.g. `claude-code/abc-123#7`).
//!
//! Refs are designed to be:
//! - **Opaque-stable across reindex**: rebuilding the search index does not
//!   change the ref for a given message. The provider slug and session id
//!   are intrinsic to the source data; the turn is the 1-based index of the
//!   message within the session in load order.
//! - **Round-trippable**: `parse(format(r)) == r` for every well-formed ref.
//! - **Human-quotable**: the form fits inline in chat / docs / commit
//!   messages without escaping.
//!
//! Turns are 1-based. Turn `0` is rejected at parse time.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use thiserror::Error;

use super::provider::Provider;
use super::session::SessionId;

/// Split an optional source prefix from a ref.
///
/// A prefix only counts as a source when `:` appears before the provider `/`.
/// This keeps unqualified session ids containing `:` round-trippable.
pub fn split_source_prefix(raw: &str) -> (Option<&str>, &str) {
    let slash = raw.find('/');
    let colon = raw.find(':');
    match (colon, slash) {
        (Some(c), Some(s)) if c > 0 && c < s => (Some(&raw[..c]), &raw[c + 1..]),
        _ => (None, raw),
    }
}

/// A stable reference to a session: `<provider-slug>/<session-id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SessionRef {
    pub provider: Provider,
    pub session_id: SessionId,
}

impl SessionRef {
    pub fn new(provider: Provider, session_id: SessionId) -> Option<Self> {
        if session_id.0.is_empty() {
            return None;
        }
        Some(Self {
            provider,
            session_id,
        })
    }

    pub fn turn(&self, turn: u32) -> Option<CitationRef> {
        CitationRef::new(self.provider, self.session_id.clone(), turn)
    }
}

impl fmt::Display for SessionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider.slug(), self.session_id)
    }
}

/// A stable reference to a single message: `<provider-slug>/<session-id>#<turn>`.
///
/// See the [module docs](self) for format guarantees.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct CitationRef {
    pub provider: Provider,
    pub session_id: SessionId,
    /// 1-based turn number within the session.
    pub turn: u32,
}

impl CitationRef {
    /// Constructs a new citation ref. Returns `None` if `turn == 0` or the
    /// session id is empty (both are rejected by the parser, so accepting
    /// them via the constructor would let callers build refs that cannot
    /// round-trip).
    pub fn new(provider: Provider, session_id: SessionId, turn: u32) -> Option<Self> {
        if turn == 0 || session_id.0.is_empty() {
            return None;
        }
        Some(Self {
            provider,
            session_id,
            turn,
        })
    }

    pub fn session_ref(&self) -> SessionRef {
        SessionRef {
            provider: self.provider,
            session_id: self.session_id.clone(),
        }
    }
}

impl fmt::Display for CitationRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}#{}",
            self.provider.slug(),
            self.session_id,
            self.turn
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CitationParseError {
    #[error("citation ref is empty")]
    Empty,
    #[error("citation ref missing provider segment (expected '<provider>/<session-id>#<turn>')")]
    MissingProvider,
    #[error("citation ref missing session id (expected '<provider>/<session-id>#<turn>')")]
    MissingSessionId,
    #[error("citation ref missing turn (expected '<provider>/<session-id>#<turn>')")]
    MissingTurn,
    #[error("unknown provider slug '{0}'")]
    UnknownProvider(String),
    #[error("invalid turn '{0}' (must be a positive integer)")]
    InvalidTurn(String),
}

fn parse_session_head(s: &str) -> Result<(Provider, SessionId), CitationParseError> {
    if s.is_empty() {
        return Err(CitationParseError::Empty);
    }
    let (provider_slug, session_str) = s
        .split_once('/')
        .ok_or(CitationParseError::MissingSessionId)?;
    if provider_slug.is_empty() {
        return Err(CitationParseError::MissingProvider);
    }
    if session_str.is_empty() {
        return Err(CitationParseError::MissingSessionId);
    }

    let provider = Provider::from_slug(provider_slug)
        .ok_or_else(|| CitationParseError::UnknownProvider(provider_slug.to_string()))?;
    Ok((provider, SessionId(session_str.to_string())))
}

impl FromStr for SessionRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(CitationParseError::Empty);
        }
        if s.contains('#') {
            return Err(CitationParseError::InvalidTurn(
                s.rsplit_once('#').map_or("", |(_, turn)| turn).to_string(),
            ));
        }
        let (provider, session_id) = parse_session_head(s)?;
        Ok(Self {
            provider,
            session_id,
        })
    }
}

impl FromStr for CitationRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(CitationParseError::Empty);
        }
        // Split on '#' first so a session id containing '/' (none today, but
        // be defensive) does not interfere with locating the turn.
        let (head, turn_str) = s.rsplit_once('#').ok_or(CitationParseError::MissingTurn)?;
        if turn_str.is_empty() {
            return Err(CitationParseError::MissingTurn);
        }

        let (provider, session_id) = parse_session_head(head)?;
        let turn: u32 = turn_str
            .parse()
            .map_err(|_| CitationParseError::InvalidTurn(turn_str.to_string()))?;
        if turn == 0 {
            return Err(CitationParseError::InvalidTurn(turn_str.to_string()));
        }

        Ok(Self {
            provider,
            session_id,
            turn,
        })
    }
}

/// A metadata ref accepts either a session ref or a turn-level citation ref.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum SessionOrTurnRef {
    Session(SessionRef),
    Turn(CitationRef),
}

impl SessionOrTurnRef {
    pub fn session_ref(&self) -> SessionRef {
        match self {
            Self::Session(session) => session.clone(),
            Self::Turn(citation) => citation.session_ref(),
        }
    }

    pub fn turn(&self) -> Option<u32> {
        match self {
            Self::Session(_) => None,
            Self::Turn(citation) => Some(citation.turn),
        }
    }
}

impl fmt::Display for SessionOrTurnRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(session) => session.fmt(f),
            Self::Turn(citation) => citation.fmt(f),
        }
    }
}

impl FromStr for SessionOrTurnRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains('#') {
            s.parse::<CitationRef>().map(Self::Turn)
        } else {
            s.parse::<SessionRef>().map(Self::Session)
        }
    }
}

/// A citation ref optionally qualified with a federated source name:
/// `<source>:<provider>/<session-id>#<turn>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct QualifiedCitationRef {
    pub source: Option<String>,
    pub citation: CitationRef,
}

impl QualifiedCitationRef {
    pub fn new(source: Option<String>, citation: CitationRef) -> Self {
        Self { source, citation }
    }
}

impl fmt::Display for QualifiedCitationRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "{source}:{}", self.citation)
        } else {
            self.citation.fmt(f)
        }
    }
}

impl FromStr for QualifiedCitationRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, raw_ref) = split_source_prefix(s);
        let citation = raw_ref.parse::<CitationRef>()?;
        Ok(Self {
            source: source.map(ToOwned::to_owned),
            citation,
        })
    }
}

#[cfg(test)]
mod tests;
