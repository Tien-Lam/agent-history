//! Semantic embedding integration (`FastEmbed` + sidecar storage).
//!
//! This module owns the optional semantic-search path:
//!
//! - **Consent** (`embeddings-consent.json`): a per-index marker recording that
//!   the user has authorised the one-off model download via `--accept-download`.
//!   Without consent, embedding generation is skipped and search stays purely
//!   lexical.
//! - **Storage** (`embeddings.bin`): a packed binary sidecar mapping
//!   `message_id -> Vec<f32>`, persisted alongside the Tantivy index.
//! - **Embedder** (feature `embeddings`): a thin wrapper around `fastembed`'s
//!   `AllMiniLML6V2` model. Building this triggers the model download on first use.
//!
//! Hybrid scoring uses Reciprocal Rank Fusion. Cache invalidation is keyed by
//! `(message_id, sha256(text))` so a message whose content changes gets
//! re-embedded on the next index pass. A bumped `STORE_VERSION` evicts the
//! whole sidecar — readers treat older versions as a schema mismatch.

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// Slug for the default text-embedding model. Stable contract — embedding
/// stores written under this name are only valid for this model.
pub const DEFAULT_MODEL: &str = "all-minilm-l6-v2";

/// Embedding dimension for `AllMiniLML6V2`. Hard-coded so we can validate
/// stored vectors without round-tripping through fastembed.
pub const DEFAULT_DIM: u32 = 384;

/// Length of a stored content hash. SHA-256 → 32 bytes.
pub const HASH_LEN: usize = 32;
/// Stable, deterministic content hash for an embedding cache entry. Two
/// messages with identical text produce identical hashes across runs and
/// machines, which is the whole point — we use it to detect when a previously
/// embedded message's text has drifted and the cached vector is stale.
pub fn content_hash(text: &str) -> [u8; HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher.finalize().into()
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("corrupt embedding store at {path}: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error("dimension mismatch: store has {stored}, vector has {got}")]
    DimMismatch { stored: u32, got: usize },
    #[error("embedding store schema mismatch: file is v{stored}, expected v{expected}")]
    SchemaMismatch { stored: u32, expected: u32 },
    #[error("embedding store field {field} is too large: {len} bytes exceeds {max}")]
    FieldTooLarge {
        field: &'static str,
        len: usize,
        max: usize,
    },
    #[error("embedding store encoded payload is too large: {len} bytes exceeds {max}")]
    StoreTooLarge { len: usize, max: usize },
    #[cfg(feature = "embeddings")]
    #[error("fastembed error: {0}")]
    Fastembed(String),
}

mod consent;
mod store;

pub use consent::Consent;
pub use store::EmbeddingStore;

#[cfg(feature = "embeddings")]
pub use feature_gated::Embedder;

mod hybrid;

pub use hybrid::{hybrid_ready, try_hybrid_search};

#[cfg(feature = "embeddings")]
mod feature_gated {
    use super::{EmbedError, DEFAULT_DIM, DEFAULT_MODEL};
    use std::path::Path;

    /// Wraps fastembed's text-embedding pipeline. Building this triggers a
    /// network download on the first use (cached afterwards in `cache_dir`).
    pub struct Embedder {
        inner: fastembed::TextEmbedding,
        dim: u32,
    }

    impl Embedder {
        /// Create an embedder, downloading the model to `cache_dir` on first run.
        ///
        /// Caller MUST have already recorded user consent before calling this —
        /// the network download is the whole point of the `--accept-download`
        /// gate, and bypassing it here would defeat the opt-in story.
        pub fn try_new(cache_dir: &Path) -> Result<Self, EmbedError> {
            std::fs::create_dir_all(cache_dir).map_err(EmbedError::Io)?;
            let opts = fastembed::InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2)
                .with_cache_dir(cache_dir.to_path_buf())
                .with_show_download_progress(true);
            let inner = fastembed::TextEmbedding::try_new(opts)
                .map_err(|e| EmbedError::Fastembed(e.to_string()))?;
            Ok(Self {
                inner,
                dim: DEFAULT_DIM,
            })
        }

        pub fn dim(&self) -> u32 {
            self.dim
        }

        pub fn model_slug(&self) -> &'static str {
            DEFAULT_MODEL
        }

        /// Embed a batch of texts. Long inputs are truncated by fastembed's
        /// tokenizer (~256 tokens for `MiniLM`); we accept that lossiness for v1.
        pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            self.inner
                .embed(texts, None)
                .map_err(|e| EmbedError::Fastembed(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_stable_and_text_sensitive() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
        assert_ne!(content_hash("hello"), content_hash("hello "));
        assert_ne!(content_hash(""), content_hash("hello"));
    }
}
