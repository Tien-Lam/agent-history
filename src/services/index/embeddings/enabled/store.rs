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
        Ok(Some(store))
            if store.model() == embed::DEFAULT_MODEL && store.dim() == embed::DEFAULT_DIM =>
        {
            Ok((store, false))
        }
        Ok(Some(_store)) => {
            embed::EmbeddingStore::evict(index_dir).map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to evict incompatible embedding store: {e}"),
                )
            })?;
            Ok((
                embed::EmbeddingStore::create(index_dir, embed::DEFAULT_MODEL, embed::DEFAULT_DIM),
                true,
            ))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_embedding_store_evicts_model_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = embed::EmbeddingStore::create(dir.path(), "old-model", embed::DEFAULT_DIM);
        store
            .upsert(
                "msg",
                embed::content_hash("hello"),
                vec![0.0; embed::DEFAULT_DIM as usize],
            )
            .unwrap();
        store.flush().unwrap();

        let (opened, evicted) = open_embedding_store(dir.path()).unwrap();

        assert!(evicted);
        assert_eq!(opened.model(), embed::DEFAULT_MODEL);
        assert_eq!(opened.dim(), embed::DEFAULT_DIM);
        assert_eq!(opened.len(), 0);
    }

    #[test]
    fn open_embedding_store_evicts_dim_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = embed::EmbeddingStore::create(dir.path(), embed::DEFAULT_MODEL, 3);
        store
            .upsert("msg", embed::content_hash("hello"), vec![0.0; 3])
            .unwrap();
        store.flush().unwrap();

        let (opened, evicted) = open_embedding_store(dir.path()).unwrap();

        assert!(evicted);
        assert_eq!(opened.model(), embed::DEFAULT_MODEL);
        assert_eq!(opened.dim(), embed::DEFAULT_DIM);
        assert_eq!(opened.len(), 0);
    }
}
