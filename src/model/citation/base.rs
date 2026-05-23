use std::fmt;

use serde::Serialize;

use crate::model::provider::Provider;
use crate::model::session::SessionId;

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct CitationRef {
    pub provider: Provider,
    pub session_id: SessionId,
    /// 1-based turn number within the session.
    pub turn: u32,
}

impl CitationRef {
    /// Constructs a new citation ref. Returns `None` if `turn == 0` or the
    /// session id is empty, so callers cannot build refs that fail to parse.
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
