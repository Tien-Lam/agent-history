use crate::model::{Provider, Session};
use crate::{federated, provider, query_scope};

use super::{IndexingError, UnfilteredIndexScope};

pub(super) struct IndexDiscovery<'a> {
    pub(super) active: Vec<&'a dyn provider::HistoryProvider>,
    pub(super) sessions: Vec<Session>,
    pub(super) errors: Vec<IndexingError>,
}

pub(super) fn discover_index_sessions<'a>(
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

pub(super) fn provider_slugs(
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
