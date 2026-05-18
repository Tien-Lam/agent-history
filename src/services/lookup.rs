use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::cli_error::ErrorEnvelope;
use crate::federated::{FederatedDiscovery, LOCAL_SOURCE};
use crate::model::{CitationRef, Message, Provider, QualifiedCitationRef, Session};
use crate::provider::{self, HistoryProvider};
use crate::session_resolver::{LookupSource, SelectorShape, SessionResolver};

#[derive(Debug)]
pub struct LoadedSession {
    pub session: Session,
    pub source: String,
    pub session_ref: String,
    pub messages: Vec<Message>,
}

#[derive(Debug)]
pub struct LoadedCitationWindow {
    pub session: Session,
    pub source: String,
    pub citation: CitationRef,
    pub citation_ref: String,
    pub messages: Vec<Message>,
    pub start_idx: usize,
    pub target_idx: usize,
    pub total_messages: usize,
}

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

fn load_messages(
    providers: &[Box<dyn HistoryProvider>],
    session: &Session,
    display_ref: &str,
) -> Result<Vec<Message>, ErrorEnvelope> {
    provider::load_messages_for_session(session, providers).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {display_ref}: {e}"),
        )
    })
}

fn citation_window(
    session: Session,
    source: String,
    citation: CitationRef,
    citation_ref: String,
    messages: Vec<Message>,
    include_context: usize,
) -> Result<LoadedCitationWindow, ErrorEnvelope> {
    let total = messages.len();
    let turn = citation.turn as usize;
    if turn == 0 || turn > total {
        return Err(ErrorEnvelope::new(
            "session-not-found",
            format!("turn {turn} out of range: session has {total} message(s)"),
        )
        .with_hint("Use `aghist export` to inspect the full session, or pick a smaller turn."));
    }

    let target_idx = turn - 1;
    let start_idx = target_idx.saturating_sub(include_context);
    let end_idx = (target_idx + include_context + 1).min(total);
    let messages = messages
        .into_iter()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .collect();
    Ok(LoadedCitationWindow {
        session,
        source,
        citation,
        citation_ref,
        messages,
        start_idx,
        target_idx,
        total_messages: total,
    })
}

fn ensure_provider_visible<S: BuildHasher>(
    provider: Option<Provider>,
    visible_providers: Option<&HashSet<Provider, S>>,
) -> Result<(), ErrorEnvelope> {
    let Some(provider) = provider else {
        return Ok(());
    };
    if visible_providers.is_none_or(|visible| visible.contains(&provider)) {
        Ok(())
    } else {
        Err(ErrorEnvelope::new(
            "provider-unavailable",
            format!(
                "provider '{}' is not enabled or not visible to MCP",
                provider.slug()
            ),
        ))
    }
}
