use std::collections::HashSet;

use crate::cli_error::ErrorEnvelope;
use crate::model::Session;
use crate::{provider, query_scope, search};

use super::{IndexingOptions, UnfilteredIndexScope};

pub(super) fn force_clear_index(
    index: &search::SearchIndex,
    scope: &query_scope::QueryScope,
    options: IndexingOptions,
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

pub(super) fn build_index(
    index: &search::SearchIndex,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    options: IndexingOptions,
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
