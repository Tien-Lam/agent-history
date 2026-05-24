use serde::Serialize;

use crate::cli_error::ErrorEnvelope;
use crate::indexing::{self, IndexingOptions, UnfilteredIndexScope};
use crate::model::Provider;
use crate::{provider, query_scope};

mod embeddings;

#[derive(Debug, Serialize)]
pub struct IndexSummary {
    #[serde(flatten)]
    core: indexing::IndexingSummary,
    embeddings: serde_json::Value,
}

impl IndexSummary {
    pub fn has_errors(&self) -> bool {
        !self.core.errors.is_empty()
    }
}

pub fn build_index_summary(
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
