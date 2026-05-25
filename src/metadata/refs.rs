use crate::config::validate_source_name;
use crate::model::{split_source_prefix, CitationParseError, SessionOrTurnRef};
use crate::schema_fragments::REFERENCE_MAX_BYTES;

use super::MetadataError;

/// Validate a `session_ref` string used as a note/tag/star key. Accepts
/// `<provider-slug>/<session-id>` (session-level),
/// `<provider-slug>/<session-id>#<turn>` (turn-level), or the same refs
/// prefixed with a registered-source-style name:
/// `<source>:<provider-slug>/<session-id>[#<turn>]`.
///
/// The provider slug must match a known [`Provider`]; the session id must be
/// non-empty; if a turn is present it must parse as a positive integer.
pub fn validate_session_ref(raw: &str) -> std::result::Result<&str, MetadataError> {
    let invalid = |reason: &'static str| MetadataError::InvalidSessionRef(raw.to_string(), reason);
    if raw.len() > REFERENCE_MAX_BYTES {
        return Err(MetadataError::SessionRefTooLong {
            bytes: raw.len(),
            max_bytes: REFERENCE_MAX_BYTES,
        });
    }
    let (source, unqualified) = split_source_prefix(raw);
    if let Some(source) = source {
        validate_source_name(source).map_err(|_message| invalid("invalid source name"))?;
    }
    unqualified.parse::<SessionOrTurnRef>().map_err(|e| {
        let reason = match e {
            CitationParseError::Empty => "empty",
            CitationParseError::MissingProvider
            | CitationParseError::MissingSessionId
            | CitationParseError::MissingTurn => "expected '<provider>/<session-id>[#<turn>]'",
            CitationParseError::InvalidSessionId(_) => {
                "session id must not contain control characters"
            }
            CitationParseError::UnknownProvider(_) => "unknown provider slug",
            CitationParseError::InvalidTurn(_) => "turn must be a positive integer",
            CitationParseError::TooLong { .. } => "reference exceeds byte limit",
        };
        invalid(reason)
    })?;
    Ok(raw)
}

/// Validate a session-or-turn ref and return the session-level key used for
/// matching annotations to sessions. The source prefix, when present, is
/// preserved.
pub fn session_key_from_ref(raw: &str) -> std::result::Result<String, MetadataError> {
    validate_session_ref(raw)?;
    Ok(raw
        .rsplit_once('#')
        .map_or(raw, |(session_ref, _turn)| session_ref)
        .to_string())
}

pub(super) fn turn_prefix_like_pattern(session_ref: &str) -> String {
    let mut pattern = escape_like(session_ref);
    pattern.push_str("#%");
    pattern
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}
