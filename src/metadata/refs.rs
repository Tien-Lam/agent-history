use crate::model::{CitationParseError, SessionOrTurnRef};

use super::MetadataError;

/// Validate a `session_ref` string used as a note key. Accepts
/// `<provider-slug>/<session-id>` (session-level) or
/// `<provider-slug>/<session-id>#<turn>` (turn-level). The provider slug must
/// match a known [`Provider`]; the session id must be non-empty; if a turn is
/// present it must parse as a positive integer.
pub fn validate_session_ref(raw: &str) -> std::result::Result<&str, MetadataError> {
    let invalid = |reason: &'static str| MetadataError::InvalidSessionRef(raw.to_string(), reason);
    raw.parse::<SessionOrTurnRef>().map_err(|e| {
        let reason = match e {
            CitationParseError::Empty => "empty",
            CitationParseError::MissingProvider
            | CitationParseError::MissingSessionId
            | CitationParseError::MissingTurn => "expected '<provider>/<session-id>[#<turn>]'",
            CitationParseError::UnknownProvider(_) => "unknown provider slug",
            CitationParseError::InvalidTurn(_) => "turn must be a positive integer",
        };
        invalid(reason)
    })?;
    Ok(raw)
}
