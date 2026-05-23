use std::path::PathBuf;

use serde::Serialize;

use crate::cli_error::ErrorEnvelope;
use crate::model::{Provider, Session};
use crate::{federated, provider, query_scope, search};

mod discovery;
mod execution;

use discovery::{discover_index_sessions, provider_slugs};
use execution::{build_index, force_clear_index};

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
        force_clear_index(&index, scope, options)?;
    }

    let stats = build_index(&index, &discovery.sessions, providers, scope, options)?;
    let mut errors = discovery.errors;
    errors.extend(
        stats
            .load_errors
            .iter()
            .map(|load_error| IndexingError::Provider {
                provider: load_error.provider.slug().to_string(),
                error: format!(
                    "failed to load session {}: {}",
                    load_error.session_id, load_error.error
                ),
            }),
    );

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
        errors,
    };

    Ok(IndexingOutcome {
        summary,
        sessions: discovery.sessions,
        index_dir,
    })
}
