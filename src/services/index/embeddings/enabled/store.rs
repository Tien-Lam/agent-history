use std::path::Path;

use crate::cli_error::ErrorEnvelope;
use crate::embed;

pub(super) fn open_embedding_store(
    index_dir: &Path,
) -> Result<(embed::EmbeddingStore, bool), ErrorEnvelope> {
    // On a schema bump (STORE_VERSION mismatch), evict the old sidecar and
    // start fresh. Refusing to reindex would be worse UX than transparently
    // rebuilding, and the JSON summary still surfaces the eviction.
    match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(store)) => Ok((store, false)),
        Ok(None) => Ok((
            embed::EmbeddingStore::create(index_dir, embed::DEFAULT_MODEL, embed::DEFAULT_DIM),
            false,
        )),
        Err(embed::EmbedError::SchemaMismatch { .. }) => {
            embed::EmbeddingStore::evict(index_dir).map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to evict outdated embedding store: {e}"),
                )
            })?;
            Ok((
                embed::EmbeddingStore::create(index_dir, embed::DEFAULT_MODEL, embed::DEFAULT_DIM),
                true,
            ))
        }
        Err(e) => Err(ErrorEnvelope::new(
            "embed-error",
            format!("failed to open embedding store: {e}"),
        )),
    }
}
