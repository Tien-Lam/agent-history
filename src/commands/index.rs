use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::indexing::{self, IndexingOptions, UnfilteredIndexScope};
use aghist::model::Provider;
use aghist::output::write_json_line;
use aghist::{provider, query_scope};
use serde::Serialize;

mod embeddings;

#[derive(Serialize)]
struct IndexSummary {
    #[serde(flatten)]
    core: indexing::IndexingSummary,
    embeddings: serde_json::Value,
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
    let outcome = indexing::run_indexing(
        providers,
        scope,
        IndexingOptions {
            provider_filter: filter,
            force,
            unfiltered_scope: UnfilteredIndexScope::AllProviders,
        },
    )?;

    #[cfg(feature = "embeddings")]
    let embed_summary = embeddings::run_embeddings(
        &outcome.index_dir,
        &outcome.sessions,
        providers,
        accept_download,
    )?;
    #[cfg(not(feature = "embeddings"))]
    let embed_summary = embeddings::disabled_embeddings_summary(accept_download);

    Ok(IndexSummary {
        core: outcome.summary,
        embeddings: embed_summary,
    })
}

fn write_index_summary(summary: &IndexSummary) -> Result<(), ErrorEnvelope> {
    let mut out = std::io::stdout().lock();
    write_json_line(&mut out, summary)
        .map_err(|e| ErrorEnvelope::io("failed to write index output", e))
}
