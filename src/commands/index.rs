use std::collections::HashSet;
use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::model::{Provider, Session};
use aghist::{federated, provider, query_scope, search};
use serde::Serialize;

mod embeddings;

#[derive(Serialize)]
struct IndexSummary {
    providers: Vec<String>,
    sessions_total: usize,
    added: usize,
    updated: usize,
    unchanged: usize,
    removed: usize,
    messages_indexed: usize,
    force: bool,
    index_dir: String,
    duration_ms: u64,
    errors: Vec<IndexError>,
    embeddings: serde_json::Value,
}

#[derive(Serialize)]
#[serde(untagged)]
enum IndexError {
    Provider { provider: String, error: String },
    Source { source: String, error: String },
}

struct IndexDiscovery<'a> {
    active: Vec<&'a dyn provider::HistoryProvider>,
    sessions: Vec<Session>,
    errors: Vec<IndexError>,
}

pub(crate) fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let summary = build_index_summary(providers, scope, filter, force, accept_download)?;
    write_index_summary(&summary)?;
    Ok(EXIT_OK)
}

fn build_index_summary(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<IndexSummary, ErrorEnvelope> {
    let started = std::time::Instant::now();
    if let Some(want) = filter {
        if !scope.contains_provider(want) {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!("provider '{}' is not enabled in config", want.slug()),
            )
            .with_hint(
                "Add the provider to `[providers].enabled`, or remove the --provider filter.",
            ));
        }
    }

    let discovery = discover_index_sessions(providers, scope, filter);

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open index at {}: {e}", index_dir.display()),
        )
    })?;
    if force {
        if let Some(want) = filter {
            let prune_providers = HashSet::from([want]);
            index.clear_providers(&prune_providers).map_err(|e| {
                ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))
            })?;
        } else {
            index.clear().map_err(|e| {
                ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))
            })?;
        }
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    // build_index needs the full provider list for load_messages dispatch;
    // provider filtering is enforced by only feeding it sessions from `active`.
    let stats = if let Some(want) = filter {
        let prune_providers = HashSet::from([want]);
        index.build_index_for_providers(&discovery.sessions, providers, &tx, &prune_providers)
    } else {
        index.build_index(&discovery.sessions, providers, &tx)
    }
    .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to build index: {e}")))?;

    #[cfg(feature = "embeddings")]
    let embed_summary =
        embeddings::run_embeddings(&index_dir, &discovery.sessions, providers, accept_download)?;
    #[cfg(not(feature = "embeddings"))]
    let embed_summary = embeddings::disabled_embeddings_summary(accept_download);

    Ok(IndexSummary {
        providers: provider_slugs(filter, &discovery.active, &discovery.sessions),
        sessions_total: discovery.sessions.len(),
        added: stats.added,
        updated: stats.updated,
        unchanged: stats.unchanged,
        removed: stats.removed,
        messages_indexed: stats.messages_indexed,
        force,
        index_dir: index_dir.display().to_string(),
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        errors: discovery.errors,
        embeddings: embed_summary,
    })
}

fn write_index_summary(summary: &IndexSummary) -> Result<(), ErrorEnvelope> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &summary).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write index output: {e}"))
    })?;
    writeln!(out).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write index output: {e}"))
    })?;
    Ok(())
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
            Err(e) => errors.push(IndexError::Provider {
                provider: p.provider().slug().to_string(),
                error: e.to_string(),
            }),
        }
    }
    errors.extend(
        append_remote_sessions(&mut sessions, scope, filter)
            .into_iter()
            .map(|failure| IndexError::Source {
                source: failure.source,
                error: failure.message,
            }),
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
    active: &[&dyn provider::HistoryProvider],
    sessions: &[Session],
) -> Vec<String> {
    if let Some(want) = filter {
        return vec![want.slug().to_string()];
    }

    Provider::all()
        .iter()
        .copied()
        .filter(|provider| {
            active.iter().any(|p| p.provider() == *provider)
                || sessions.iter().any(|session| session.provider == *provider)
        })
        .map(Provider::slug)
        .map(str::to_string)
        .collect()
}
