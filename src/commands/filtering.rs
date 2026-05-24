use std::collections::HashSet;

use aghist::cli_error::ErrorEnvelope;
use aghist::metadata;
use aghist::model::{Message, Session};
use aghist::session_resolver;

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
    filters
        .to_search_filters()
        .matches_session_with_project_needle(session, project_needle)
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

/// Drop a `#<turn>` suffix, preserving any source prefix and leaving the same
/// session-level key shape as the canonical session metadata key so note refs
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
    session_resolver::metadata_filter_matches_source(session, source, metadata_keys)
}

pub(crate) fn message_matches(message: &Message, filters: &FilterArgs) -> bool {
    filters.to_search_filters().matches_message(message)
}
