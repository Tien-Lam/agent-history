use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::fs_atomic;
use crate::fs_read;

use super::{EmbedError, HASH_LEN};

mod codec;

#[cfg(test)]
mod tests;

use codec::{decode, encode, MAX_EMBEDDING_STORE_BYTES};

const STORE_FILENAME: &str = "embeddings.bin";

/// One stored entry: the content hash that produced this vector, plus the
/// vector itself. Splitting these out makes freshness checks a hash compare
/// without touching the (much larger) `f32` payload.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) hash: [u8; HASH_LEN],
    pub(super) vector: Vec<f32>,
}

/// Packed binary sidecar mapping `message_id -> (content_hash, Vec<f32>)`.
///
/// Format (all integers little-endian):
/// ```text
/// magic[6] = b"AGEMB\0"
/// version: u32          // current = 2
/// dim:     u32
/// model_len: u32
/// model: utf8 bytes
/// records (until EOF):
///   id_len: u32
///   id:    utf8 bytes
///   hash:  32 bytes (sha256 of message text at embed time)
///   vec:   dim * f32
/// ```
///
/// The whole file is read into memory on `open` and rewritten atomically on
/// `flush`. ~4 bytes/dim means `MiniLM` (384d) costs ~1.5KB per message, which
/// is fine for the 10K-message scale we expect here.
pub struct EmbeddingStore {
    path: PathBuf,
    dim: u32,
    model: String,
    entries: HashMap<String, Entry>,
}

impl EmbeddingStore {
    /// Open an existing store, or `None` if no file is present.
    pub fn open(index_dir: &Path) -> Result<Option<Self>, EmbedError> {
        let path = index_dir.join(STORE_FILENAME);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs_read::read_limited(&path, MAX_EMBEDDING_STORE_BYTES)?;
        let decoded = decode(&path, &bytes)?;
        Ok(Some(Self {
            path,
            dim: decoded.dim,
            model: decoded.model,
            entries: decoded.entries,
        }))
    }

    /// Create an empty store. Not flushed until [`flush`](Self::flush) is called.
    pub fn create(index_dir: &Path, model: &str, dim: u32) -> Self {
        Self {
            path: index_dir.join(STORE_FILENAME),
            dim,
            model: model.to_string(),
            entries: HashMap::new(),
        }
    }

    /// Delete the on-disk sidecar (if any). Used by callers when an open
    /// returned [`EmbedError::SchemaMismatch`] and they want to start fresh
    /// rather than refuse to reindex.
    pub fn evict(index_dir: &Path) -> Result<(), EmbedError> {
        let path = index_dir.join(STORE_FILENAME);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(EmbedError::Io(e)),
        }
    }

    pub fn dim(&self) -> u32 {
        self.dim
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Diagnostic accessor — returns the stored vector regardless of whether
    /// its hash still matches current text. Prefer
    /// [`get_if_fresh`](Self::get_if_fresh) for cache-hit checks.
    pub fn get(&self, message_id: &str) -> Option<&[f32]> {
        self.entries.get(message_id).map(|e| e.vector.as_slice())
    }

    /// Iterate over `(message_id, vector)` pairs. Order is unspecified —
    /// callers that need stable order should sort downstream. Used by hybrid
    /// search to compute cosine similarity against every cached embedding.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[f32])> {
        self.entries
            .iter()
            .map(|(id, entry)| (id.as_str(), entry.vector.as_slice()))
    }

    /// Returns the stored vector iff its hash matches `expected_hash`. A
    /// `None` here means "either never embedded, or the message text has
    /// changed since" — both cases require a fresh embedding pass.
    pub fn get_if_fresh(&self, message_id: &str, expected_hash: &[u8; HASH_LEN]) -> Option<&[f32]> {
        let entry = self.entries.get(message_id)?;
        if &entry.hash == expected_hash {
            Some(entry.vector.as_slice())
        } else {
            None
        }
    }

    pub fn upsert(
        &mut self,
        message_id: &str,
        hash: [u8; HASH_LEN],
        vector: Vec<f32>,
    ) -> Result<(), EmbedError> {
        if u32::try_from(vector.len()).is_ok_and(|n| n == self.dim) {
            self.entries
                .insert(message_id.to_string(), Entry { hash, vector });
            Ok(())
        } else {
            Err(EmbedError::DimMismatch {
                stored: self.dim,
                got: vector.len(),
            })
        }
    }

    /// Drop entries whose message keys are no longer present in the lexical
    /// index source set. Returns the number of removed vectors.
    pub fn retain_keys(&mut self, live_keys: &HashSet<String>) -> usize {
        let before = self.entries.len();
        self.entries.retain(|key, _| live_keys.contains(key));
        before - self.entries.len()
    }

    /// Drop stale entries whose keys belong to the caller-selected scope.
    /// Entries outside the scope are left untouched so provider-scoped index
    /// runs do not erase embeddings for providers that were not scanned.
    pub fn retain_scoped_keys(
        &mut self,
        live_keys: &HashSet<String>,
        mut is_in_prune_scope: impl FnMut(&str) -> bool,
    ) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|key, _| !is_in_prune_scope(key) || live_keys.contains(key));
        before - self.entries.len()
    }

    /// Atomically rewrite the sidecar through a sibling temp file so a crash
    /// mid-write can't corrupt an existing store.
    pub fn flush(&self) -> Result<(), EmbedError> {
        let bytes = encode(self.dim, &self.model, &self.entries)?;
        fs_atomic::write(&self.path, &bytes)?;
        Ok(())
    }
}
