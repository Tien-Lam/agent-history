use crate::model::Provider;

#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    /// Sessions written this pass (added + updated).
    pub sessions_indexed: usize,
    /// Messages written this pass.
    pub messages_indexed: usize,
    /// Sessions never seen by the manifest before.
    pub added: usize,
    /// Sessions that existed in the manifest but had a newer source mtime.
    pub updated: usize,
    /// Sessions that the manifest already had at the current mtime - skipped.
    pub unchanged: usize,
    /// Sessions present in the manifest but no longer discovered this pass.
    pub removed: usize,
    /// Discovered sessions that could not be loaded and therefore were not indexed.
    pub load_errors: Vec<IndexLoadError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexLoadError {
    pub provider: Provider,
    pub session_id: String,
    pub error: String,
}

/// Outcome of a single index-notes pass. Mirrors [`IndexStats`] in spirit but
/// counts metadata.db note rows instead of session files.
#[derive(Debug, Default, Clone)]
pub struct NotesIndexStats {
    /// Notes never seen by the manifest before.
    pub added: usize,
    /// Notes whose `updated_at` advanced since the manifest snapshot.
    pub updated: usize,
    /// Notes already present at the current `updated_at` - skipped.
    pub unchanged: usize,
    /// Notes present in the manifest but absent from the input list - pruned
    /// from the index so deletes in the sidecar propagate to search.
    pub removed: usize,
}
