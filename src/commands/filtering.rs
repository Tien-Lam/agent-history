use std::collections::HashSet;

use aghist::cli_error::ErrorEnvelope;
use aghist::federated;
use aghist::metadata;
use aghist::model::{ContentBlock, Message, Session};

use super::super::cli::FilterArgs;
use super::metadata::{metadata_error, open_metadata_db};

/// Apply session-level filters (provider, since/until, project). Provider is
/// not re-checked here when the caller already filtered by provider, but it's
/// harmless to do so. `project_needle` is the pre-lowercased substring for
/// efficiency in the per-session loop.
pub(crate) fn session_matches(
    session: &Session,
    filters: &FilterArgs,
    project_needle: Option<&str>,
) -> bool {
    if let Some(want) = filters.provider {
        if session.provider != want {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if session.started_at < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if session.started_at > until {
            return false;
        }
    }
    if let Some(needle) = project_needle {
        let project = session
            .project_name
            .as_deref()
            .map(str::to_lowercase)
            .unwrap_or_default();
        if !project.contains(needle) {
            return false;
        }
    }
    true
}

/// Resolve `--note`/`--tag`/`--starred` into a set of
/// `<provider-slug>/<session-id>` keys, opening the metadata sidecar on
/// demand. Returns `Ok(None)` when no metadata filter is requested so the
/// caller can skip the lookup entirely (and avoid creating the DB on disk).
pub(crate) fn resolve_metadata_filter(
    filters: &FilterArgs,
) -> Result<Option<HashSet<String>>, ErrorEnvelope> {
    if !filters.has_metadata_filter() {
        return Ok(None);
    }
    let conn = open_metadata_db()?;
    metadata::filter_session_keys(
        &conn,
        filters.note.as_deref(),
        filters.tag.as_deref(),
        filters.starred,
    )
    .map_err(|e| metadata_error(&e))
}

/// Build the canonical metadata key for a session: `<provider-slug>/<id>`.
pub(crate) fn session_metadata_key(session: &Session) -> String {
    session.session_ref().to_string()
}

/// Build the metadata key for a federated session. Local sessions keep the
/// legacy unqualified shape; remote sessions use
/// `<source>:<provider-slug>/<id>`.
pub(crate) fn qualified_session_metadata_key(session: &Session, source: &str) -> String {
    let raw = session_metadata_key(session);
    if source == federated::LOCAL_SOURCE {
        raw
    } else {
        format!("{source}:{raw}")
    }
}

/// Drop a `#<turn>` suffix, preserving any source prefix and leaving the same
/// session-level key shape as [`qualified_session_metadata_key`] so note refs
/// and session refs can be compared against the same allow-set.
pub(crate) fn strip_turn_suffix(session_ref: &str) -> &str {
    session_ref
        .rsplit_once('#')
        .map_or(session_ref, |(prefix, _)| prefix)
}

/// Returns true when `metadata_keys` is `None` (filter inactive) or when the
/// session's source-aware metadata key is in the allowed set.
pub(crate) fn metadata_filter_matches_source(
    session: &Session,
    source: &str,
    metadata_keys: Option<&HashSet<String>>,
) -> bool {
    let Some(keys) = metadata_keys else {
        return true;
    };
    keys.contains(&qualified_session_metadata_key(session, source))
}

pub(crate) fn message_matches(message: &Message, filters: &FilterArgs) -> bool {
    if let Some(role) = filters.role {
        if message.role != role {
            return false;
        }
    }
    if filters.has_tool_call
        && !message
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse(_)))
    {
        return false;
    }
    true
}
