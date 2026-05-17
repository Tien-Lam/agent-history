use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::cli_error::ErrorEnvelope;
use crate::model::{Provider, Session};
use crate::{federated, provider, query_scope, search};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnfilteredIndexScope {
    AllProviders,
    VisibleProviders,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexingOptions {
    pub provider_filter: Option<Provider>,
    pub force: bool,
    pub unfiltered_scope: UnfilteredIndexScope,
}

#[derive(Debug)]
pub struct IndexingOutcome {
    pub summary: IndexingSummary,
    pub sessions: Vec<Session>,
    pub index_dir: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct IndexingSummary {
    pub providers: Vec<String>,
    pub sessions_total: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub messages_indexed: usize,
    pub force: bool,
    pub index_dir: String,
    pub duration_ms: u64,
    pub errors: Vec<IndexingError>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum IndexingError {
    Provider { provider: String, error: String },
    Source(federated::SourceError),
}

struct IndexDiscovery<'a> {
    active: Vec<&'a dyn provider::HistoryProvider>,
    sessions: Vec<Session>,
    errors: Vec<IndexingError>,
}

pub fn run_indexing(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    options: IndexingOptions,
) -> Result<IndexingOutcome, ErrorEnvelope> {
    let started = std::time::Instant::now();
    if let Some(want) = options.provider_filter {
        if !scope.contains_provider(want) {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "provider '{}' is not enabled or visible in this command",
                    want.slug()
                ),
            )
            .with_hint(
                "Add the provider to the relevant config allowlist, or remove the provider filter.",
            ));
        }
    }

    let discovery = discover_index_sessions(providers, scope, options.provider_filter);
    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open index at {}: {e}", index_dir.display()),
        )
    })?;

    if options.force {
        force_clear_index(&index, scope, &options)?;
    }

    let stats = build_index(&index, &discovery.sessions, providers, scope, &options)?;

    let summary = IndexingSummary {
        providers: provider_slugs(
            options.provider_filter,
            options.unfiltered_scope,
            scope,
            &discovery.active,
            &discovery.sessions,
        ),
        sessions_total: discovery.sessions.len(),
        added: stats.added,
        updated: stats.updated,
        unchanged: stats.unchanged,
        removed: stats.removed,
        messages_indexed: stats.messages_indexed,
        force: options.force,
        index_dir: index_dir.display().to_string(),
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        errors: discovery.errors,
    };

    Ok(IndexingOutcome {
        summary,
        sessions: discovery.sessions,
        index_dir,
    })
}

fn force_clear_index(
    index: &search::SearchIndex,
    scope: &query_scope::QueryScope,
    options: &IndexingOptions,
) -> Result<(), ErrorEnvelope> {
    if let Some(want) = options.provider_filter {
        let prune_providers = HashSet::from([want]);
        return index
            .clear_providers(&prune_providers)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to clear index: {e}")));
    }

    match options.unfiltered_scope {
        UnfilteredIndexScope::AllProviders => index
            .clear()
            .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))),
        UnfilteredIndexScope::VisibleProviders => index
            .clear_providers(scope.providers())
            .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))),
    }
}

fn build_index(
    index: &search::SearchIndex,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    options: &IndexingOptions,
) -> Result<search::IndexStats, ErrorEnvelope> {
    let (tx, _rx) = crossbeam_channel::unbounded();
    // SearchIndex still needs the full provider list for message-loading
    // dispatch; the session set and pruning scope enforce the selected view.
    let result = if let Some(want) = options.provider_filter {
        let prune_providers = HashSet::from([want]);
        index.build_index_for_providers(sessions, providers, &tx, &prune_providers)
    } else {
        match options.unfiltered_scope {
            UnfilteredIndexScope::AllProviders => index.build_index(sessions, providers, &tx),
            UnfilteredIndexScope::VisibleProviders => {
                index.build_index_for_providers(sessions, providers, &tx, scope.providers())
            }
        }
    };
    result.map_err(|e| ErrorEnvelope::new("index-error", format!("failed to build index: {e}")))
}

fn discover_index_sessions<'a>(
    providers: &'a [Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filter: Option<Provider>,
) -> IndexDiscovery<'a> {
    let active: Vec<&dyn provider::HistoryProvider> = providers
        .iter()
        .map(Box::as_ref)
        .filter(|p| scope.contains_provider(p.provider()))
        .filter(|p| filter.is_none_or(|want| p.provider() == want))
        .collect();

    let mut sessions = Vec::new();
    let mut errors = Vec::new();
    for p in &active {
        match p.discover_sessions() {
            Ok(found) => sessions.extend(found),
            Err(e) => errors.push(IndexingError::Provider {
                provider: p.provider().slug().to_string(),
                error: e.to_string(),
            }),
        }
    }
    errors.extend(
        append_remote_sessions(&mut sessions, scope, filter)
            .into_iter()
            .map(|failure| IndexingError::Source(failure.to_error())),
    );

    IndexDiscovery {
        active,
        sessions,
        errors,
    }
}

fn append_remote_sessions(
    sessions: &mut Vec<Session>,
    scope: &query_scope::QueryScope,
    filter: Option<Provider>,
) -> Vec<federated::SourceFailure> {
    let Some(remote) = scope.discover_remote_sources() else {
        return Vec::new();
    };

    sessions.extend(
        remote
            .sessions
            .into_iter()
            .filter(|session| filter.is_none_or(|want| session.provider == want)),
    );
    remote.failures
}

fn provider_slugs(
    filter: Option<Provider>,
    unfiltered_scope: UnfilteredIndexScope,
    scope: &query_scope::QueryScope,
    active: &[&dyn provider::HistoryProvider],
    sessions: &[Session],
) -> Vec<String> {
    if let Some(want) = filter {
        return vec![want.slug().to_string()];
    }

    Provider::all()
        .iter()
        .copied()
        .filter(|provider| match unfiltered_scope {
            UnfilteredIndexScope::AllProviders => {
                active.iter().any(|p| p.provider() == *provider)
                    || sessions.iter().any(|session| session.provider == *provider)
            }
            UnfilteredIndexScope::VisibleProviders => {
                scope.contains_provider(*provider)
                    && sessions.iter().any(|session| session.provider == *provider)
            }
        })
        .map(Provider::slug)
        .map(str::to_string)
        .collect()
}
