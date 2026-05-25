use std::str::FromStr;

use thiserror::Error;

use super::base::session_id_is_valid;
use super::{CitationRef, SessionOrTurnRef, SessionRef};
use crate::model::provider::Provider;
use crate::model::session::SessionId;
use crate::schema_fragments::REFERENCE_MAX_BYTES;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CitationParseError {
    #[error("citation ref is empty")]
    Empty,
    #[error("citation ref missing provider segment (expected '<provider>/<session-id>#<turn>')")]
    MissingProvider,
    #[error("citation ref missing session id (expected '<provider>/<session-id>#<turn>')")]
    MissingSessionId,
    #[error("invalid session id '{0}' (must not contain control characters)")]
    InvalidSessionId(String),
    #[error("citation ref missing turn (expected '<provider>/<session-id>#<turn>')")]
    MissingTurn,
    #[error("unknown provider slug '{0}'")]
    UnknownProvider(String),
    #[error("invalid turn '{0}' (must be a positive integer)")]
    InvalidTurn(String),
    #[error("reference exceeds {max_bytes} byte limit ({bytes} bytes)")]
    TooLong { bytes: usize, max_bytes: usize },
}

fn enforce_ref_length(s: &str) -> Result<(), CitationParseError> {
    if s.len() > REFERENCE_MAX_BYTES {
        return Err(CitationParseError::TooLong {
            bytes: s.len(),
            max_bytes: REFERENCE_MAX_BYTES,
        });
    }
    Ok(())
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
    if !session_id_is_valid(session_str) {
        return Err(CitationParseError::InvalidSessionId(
            session_str.to_string(),
        ));
    }

    let provider = Provider::from_slug(provider_slug)
        .ok_or_else(|| CitationParseError::UnknownProvider(provider_slug.to_string()))?;
    Ok((provider, SessionId(session_str.to_string())))
}

impl FromStr for SessionRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        enforce_ref_length(s)?;
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
        enforce_ref_length(s)?;
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

impl FromStr for SessionOrTurnRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        enforce_ref_length(s)?;
        if s.contains('#') {
            s.parse::<CitationRef>().map(Self::Turn)
        } else {
            s.parse::<SessionRef>().map(Self::Session)
        }
    }
}
