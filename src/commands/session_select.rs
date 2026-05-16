use std::collections::HashMap;

use aghist::cli_error::ErrorEnvelope;
use aghist::model::Session;
use aghist::session_resolver::{self, SessionResolver};

pub(crate) use session_resolver::{SelectedCitation, SelectedSession, SelectorShape};

pub(crate) fn resolve_session_selector<'a>(
    sessions: &'a [Session],
    source_by_session: &'a HashMap<String, String>,
    selector: &str,
    shape: SelectorShape,
) -> Result<SelectedSession<'a>, ErrorEnvelope> {
    SessionResolver::new(sessions, source_by_session)
        .resolve_session_selector(selector, shape)
        .map_err(ErrorEnvelope::from)
}

pub(crate) fn resolve_citation_selector<'a>(
    sessions: &'a [Session],
    source_by_session: &'a HashMap<String, String>,
    selector: &str,
) -> Result<SelectedCitation<'a>, ErrorEnvelope> {
    SessionResolver::new(sessions, source_by_session)
        .resolve_citation_selector(selector)
        .map_err(ErrorEnvelope::from)
}
