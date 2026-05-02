//! Stable citation references for individual messages within a session.
//!
//! A [`CitationRef`] identifies one message via the triple
//! `(provider, session_id, turn)`, formatted as
//! `<provider-slug>/<session-id>#<turn>` (e.g. `claude-code/abc-123#7`).
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
        Some(Self { provider, session_id, turn })
    }
}

impl fmt::Display for CitationRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}#{}", self.provider.slug(), self.session_id, self.turn)
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

impl FromStr for CitationRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(CitationParseError::Empty);
        }

        // Split on '#' first so a session id containing '/' (none today, but
        // be defensive) does not interfere with locating the turn.
        let (head, turn_str) = s
            .rsplit_once('#')
            .ok_or(CitationParseError::MissingTurn)?;
        if turn_str.is_empty() {
            return Err(CitationParseError::MissingTurn);
        }

        let (provider_slug, session_str) = head
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

        let turn: u32 = turn_str
            .parse()
            .map_err(|_| CitationParseError::InvalidTurn(turn_str.to_string()))?;
        if turn == 0 {
            return Err(CitationParseError::InvalidTurn(turn_str.to_string()));
        }

        Ok(Self {
            provider,
            session_id: SessionId(session_str.to_string()),
            turn,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(s: &str) -> SessionId {
        SessionId(s.to_string())
    }

    #[test]
    fn display_uses_provider_slug() {
        let r = CitationRef {
            provider: Provider::ClaudeCode,
            session_id: sid("abc-123"),
            turn: 7,
        };
        assert_eq!(r.to_string(), "claude-code/abc-123#7");
    }

    #[test]
    fn parse_basic() {
        let r: CitationRef = "claude-code/abc-123#7".parse().unwrap();
        assert_eq!(r.provider, Provider::ClaudeCode);
        assert_eq!(r.session_id, sid("abc-123"));
        assert_eq!(r.turn, 7);
    }

    #[test]
    fn round_trip_all_providers() {
        for &p in Provider::all() {
            let original = CitationRef {
                provider: p,
                session_id: sid("ses-uuid-0001"),
                turn: 42,
            };
            let rendered = original.to_string();
            let parsed: CitationRef = rendered.parse().unwrap();
            assert_eq!(parsed, original, "round-trip for {p:?} ({rendered})");
        }
    }

    #[test]
    fn round_trip_complex_session_ids() {
        let ids = [
            "rollout-2024-03-15T10-30-00-a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "ses_abc123",
            "uuid-789-with-many-segments-and-longer-tail-0001",
        ];
        for id in ids {
            let original = CitationRef {
                provider: Provider::CodexCli,
                session_id: sid(id),
                turn: 1,
            };
            let parsed: CitationRef = original.to_string().parse().unwrap();
            assert_eq!(parsed, original);
        }
    }

    #[test]
    fn parse_rejects_empty() {
        assert_eq!("".parse::<CitationRef>(), Err(CitationParseError::Empty));
    }

    #[test]
    fn parse_rejects_missing_turn() {
        assert_eq!(
            "claude-code/abc".parse::<CitationRef>(),
            Err(CitationParseError::MissingTurn)
        );
        assert_eq!(
            "claude-code/abc#".parse::<CitationRef>(),
            Err(CitationParseError::MissingTurn)
        );
    }

    #[test]
    fn parse_rejects_missing_session_id() {
        assert_eq!(
            "claude-code#5".parse::<CitationRef>(),
            Err(CitationParseError::MissingSessionId)
        );
        assert_eq!(
            "claude-code/#5".parse::<CitationRef>(),
            Err(CitationParseError::MissingSessionId)
        );
    }

    #[test]
    fn parse_rejects_missing_provider() {
        assert_eq!(
            "/abc#5".parse::<CitationRef>(),
            Err(CitationParseError::MissingProvider)
        );
    }

    #[test]
    fn parse_rejects_unknown_provider() {
        assert_eq!(
            "Claude-Code/abc#5".parse::<CitationRef>(),
            Err(CitationParseError::UnknownProvider("Claude-Code".into()))
        );
        assert_eq!(
            "fake-provider/abc#5".parse::<CitationRef>(),
            Err(CitationParseError::UnknownProvider("fake-provider".into()))
        );
    }

    #[test]
    fn parse_rejects_zero_turn() {
        assert_eq!(
            "claude-code/abc#0".parse::<CitationRef>(),
            Err(CitationParseError::InvalidTurn("0".into()))
        );
    }

    #[test]
    fn parse_rejects_non_numeric_turn() {
        assert_eq!(
            "claude-code/abc#seven".parse::<CitationRef>(),
            Err(CitationParseError::InvalidTurn("seven".into()))
        );
        assert_eq!(
            "claude-code/abc#-1".parse::<CitationRef>(),
            Err(CitationParseError::InvalidTurn("-1".into()))
        );
    }

    #[test]
    fn new_validates_inputs() {
        assert!(CitationRef::new(Provider::ClaudeCode, sid("abc"), 1).is_some());
        assert!(CitationRef::new(Provider::ClaudeCode, sid("abc"), 0).is_none());
        assert!(CitationRef::new(Provider::ClaudeCode, sid(""), 1).is_none());
    }
}
