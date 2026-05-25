use std::collections::HashSet;

use aghist::cli_error::ErrorEnvelope;
use aghist::metadata;
use aghist::model::{Message, Session};
use aghist::provider;
use aghist::search::SearchFilters;
use aghist::session_resolver;
use aghist::session_warnings::SessionLoadWarning;

use super::super::cli::FilterArgs;
use super::metadata::{metadata_error, open_metadata_db};

/// Prepared command filters used by report and analysis paths that scan many
/// sessions/messages. This avoids rebuilding `SearchFilters` and lowercasing
/// the project needle inside hot loops.
pub(crate) struct PreparedFilters {
    search: SearchFilters,
    project_needle: Option<String>,
}

impl PreparedFilters {
    pub(crate) fn from_args(filters: &FilterArgs) -> Self {
        let search = filters.to_search_filters();
        let project_needle = search.project_needle();
        Self {
            search,
            project_needle,
        }
    }

    pub(crate) fn matches_session(&self, session: &Session) -> bool {
        self.search
            .matches_session_with_project_needle(session, self.project_needle.as_deref())
    }

    pub(crate) fn matches_message(&self, message: &Message) -> bool {
        self.search.matches_message(message)
    }
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

pub(crate) fn load_messages_or_warn(
    providers: &[Box<dyn provider::HistoryProvider>],
    source: &str,
    session: &Session,
) -> Option<Vec<Message>> {
    match provider::load_messages_for_session(session, providers) {
        Ok(messages) => Some(messages),
        Err(error) => {
            eprintln!(
                "{}",
                SessionLoadWarning::new(source, session, error).warning_line()
            );
            None
        }
    }
}
