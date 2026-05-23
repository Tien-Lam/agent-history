use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::cli_error::ErrorEnvelope;
use crate::federated::{FederatedDiscovery, LOCAL_SOURCE};
use crate::model::{CitationRef, Provider, QualifiedCitationRef};
use crate::provider::HistoryProvider;
use crate::session_resolver::{LookupSource, SelectorShape, SessionResolver};

mod loader;
mod types;
mod visibility;
mod window;

pub use types::{LoadedCitationWindow, LoadedSession};

use loader::load_messages;
use visibility::ensure_provider_visible;
use window::citation_window;

pub fn load_session_by_selector(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &FederatedDiscovery,
    selector: &str,
    shape: SelectorShape,
) -> Result<LoadedSession, ErrorEnvelope> {
    let resolver = SessionResolver::new(&discovery.sessions, &discovery.source_by_session);
    let target = resolver
        .resolve_session_selector(selector, shape)
        .map_err(ErrorEnvelope::from)?;
    let messages = load_messages(providers, target.session, &target.session_ref)?;
    Ok(LoadedSession {
        session: target.session.clone(),
        source: target.source.to_string(),
        session_ref: target.session_ref,
        messages,
    })
}

pub fn load_citation_by_selector(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &FederatedDiscovery,
    selector: &str,
    include_context: usize,
) -> Result<LoadedCitationWindow, ErrorEnvelope> {
    let resolver = SessionResolver::new(&discovery.sessions, &discovery.source_by_session);
    let target = resolver
        .resolve_citation_selector(selector)
        .map_err(ErrorEnvelope::from)?;
    let messages = load_messages(providers, target.session, &target.citation_ref)?;
    citation_window(
        target.session.clone(),
        target.source.to_string(),
        target.citation,
        target.citation_ref,
        messages,
        include_context,
    )
}

pub fn load_session_by_prefix<S: BuildHasher>(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &FederatedDiscovery,
    session_id: &str,
    provider_filter: Option<Provider>,
    source_filter: Option<&str>,
    visible_providers: Option<&HashSet<Provider, S>>,
) -> Result<LoadedSession, ErrorEnvelope> {
    ensure_provider_visible(provider_filter, visible_providers)?;
    let resolver = SessionResolver::new(&discovery.sessions, &discovery.source_by_session);
    let source_filter = LookupSource::from_optional(source_filter).map_err(ErrorEnvelope::from)?;
    let target = resolver
        .find_by_id_prefix(session_id, provider_filter, source_filter)
        .map_err(ErrorEnvelope::from)?;
    let messages = load_messages(providers, target.session, &target.session_ref)?;
    Ok(LoadedSession {
        session: target.session.clone(),
        source: target.source.to_string(),
        session_ref: target.session_ref,
        messages,
    })
}

pub fn load_exact_session<S: BuildHasher>(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &FederatedDiscovery,
    provider: Provider,
    session_id: &str,
    source_filter: LookupSource<'_>,
    visible_providers: Option<&HashSet<Provider, S>>,
) -> Result<LoadedSession, ErrorEnvelope> {
    ensure_provider_visible(Some(provider), visible_providers)?;
    let resolver = SessionResolver::new(&discovery.sessions, &discovery.source_by_session);
    let target = resolver
        .find_exact(provider, session_id, source_filter)
        .map_err(ErrorEnvelope::from)?;
    let messages = load_messages(providers, target.session, &target.session_ref)?;
    Ok(LoadedSession {
        session: target.session.clone(),
        source: target.source.to_string(),
        session_ref: target.session_ref,
        messages,
    })
}

pub fn load_exact_citation_window<S: BuildHasher>(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &FederatedDiscovery,
    citation: CitationRef,
    source_filter: LookupSource<'_>,
    include_context: usize,
    visible_providers: Option<&HashSet<Provider, S>>,
) -> Result<LoadedCitationWindow, ErrorEnvelope> {
    let loaded = load_exact_session(
        providers,
        discovery,
        citation.provider,
        &citation.session_id.0,
        source_filter,
        visible_providers,
    )?;
    let citation_ref = QualifiedCitationRef::new(
        (loaded.source != LOCAL_SOURCE).then(|| loaded.source.clone()),
        citation.clone(),
    )
    .to_string();
    citation_window(
        loaded.session,
        loaded.source,
        citation,
        citation_ref,
        loaded.messages,
        include_context,
    )
}
