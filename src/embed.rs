//! Semantic embedding integration (`FastEmbed` + sidecar storage).
//!
//! This module is the scaffold for ahist-y3o.4 (semantic search). It owns:
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
//! Hybrid scoring (RRF) and content-hash cache invalidation live in follow-up
//! beads (ahist-y3o.4.2 / 4.3) and are intentionally out of scope here.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Slug for the default text-embedding model. Stable contract — embedding
/// stores written under this name are only valid for this model.
pub const DEFAULT_MODEL: &str = "all-minilm-l6-v2";

/// Embedding dimension for `AllMiniLML6V2`. Hard-coded so we can validate
/// stored vectors without round-tripping through fastembed.
pub const DEFAULT_DIM: u32 = 384;

const CONSENT_FILENAME: &str = "embeddings-consent.json";
const STORE_FILENAME: &str = "embeddings.bin";
const STORE_MAGIC: &[u8; 6] = b"AGEMB\0";
const STORE_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("corrupt embedding store at {path}: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error("dimension mismatch: store has {stored}, vector has {got}")]
    DimMismatch { stored: u32, got: usize },
    #[cfg(feature = "embeddings")]
    #[error("fastembed error: {0}")]
    Fastembed(String),
}

/// Records that a user has acknowledged the one-off model download for
/// `model`. Stored as JSON next to the index so re-runs don't re-prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consent {
    pub model: String,
    pub accepted_at: chrono::DateTime<chrono::Utc>,
}

impl Consent {
    pub fn path(index_dir: &Path) -> PathBuf {
        index_dir.join(CONSENT_FILENAME)
    }

    pub fn load(index_dir: &Path) -> Option<Self> {
        let raw = fs::read_to_string(Self::path(index_dir)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Writes (or refreshes) consent. Creates `index_dir` if needed.
    pub fn record(index_dir: &Path, model: &str) -> Result<Self, EmbedError> {
        fs::create_dir_all(index_dir)?;
        let consent = Self {
            model: model.to_string(),
            accepted_at: chrono::Utc::now(),
        };
        let path = Self::path(index_dir);
        let json = serde_json::to_string_pretty(&consent)?;
        fs::write(&path, json)?;
        Ok(consent)
    }
}

/// Packed binary sidecar mapping `message_id -> Vec<f32>`.
///
/// Format (all integers little-endian):
/// ```text
/// magic[6] = b"AGEMB\0"
/// version: u32          // current = 1
/// dim:     u32
/// model_len: u32
/// model: utf8 bytes
/// records (until EOF):
///   id_len: u32
///   id:    utf8 bytes
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
    vectors: HashMap<String, Vec<f32>>,
}

impl EmbeddingStore {
    /// Open an existing store, or `None` if no file is present.
    pub fn open(index_dir: &Path) -> Result<Option<Self>, EmbedError> {
        let path = index_dir.join(STORE_FILENAME);
        if !path.exists() {
            return Ok(None);
        }
        let mut file = fs::File::open(&path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let store = Self::decode(&path, &bytes)?;
        Ok(Some(store))
    }

    /// Create an empty store. Not flushed until [`flush`](Self::flush) is called.
    pub fn create(index_dir: &Path, model: &str, dim: u32) -> Self {
        Self {
            path: index_dir.join(STORE_FILENAME),
            dim,
            model: model.to_string(),
            vectors: HashMap::new(),
        }
    }

    pub fn dim(&self) -> u32 {
        self.dim
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn get(&self, message_id: &str) -> Option<&[f32]> {
        self.vectors.get(message_id).map(Vec::as_slice)
    }

    pub fn upsert(&mut self, message_id: &str, vector: Vec<f32>) -> Result<(), EmbedError> {
        if u32::try_from(vector.len()).is_ok_and(|n| n == self.dim) {
            self.vectors.insert(message_id.to_string(), vector);
            Ok(())
        } else {
            Err(EmbedError::DimMismatch {
                stored: self.dim,
                got: vector.len(),
            })
        }
    }

    /// Atomically rewrite the sidecar with the current contents. Writes to a
    /// `.tmp` file first, then renames — so a crash mid-write can't corrupt
    /// an existing store.
    pub fn flush(&self) -> Result<(), EmbedError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = self.encode();
        let tmp = self.path.with_extension("bin.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            STORE_MAGIC.len()
                + 4 * 3
                + self.model.len()
                + self.vectors.len() * (4 + 32 + (self.dim as usize) * 4),
        );
        out.extend_from_slice(STORE_MAGIC);
        out.extend_from_slice(&STORE_VERSION.to_le_bytes());
        out.extend_from_slice(&self.dim.to_le_bytes());
        let model_bytes = self.model.as_bytes();
        out.extend_from_slice(&u32_len(model_bytes).to_le_bytes());
        out.extend_from_slice(model_bytes);
        // Stable order — sorting lets snapshots and fixture tests be deterministic.
        let mut ids: Vec<&String> = self.vectors.keys().collect();
        ids.sort();
        for id in ids {
            let vec = &self.vectors[id];
            let id_bytes = id.as_bytes();
            out.extend_from_slice(&u32_len(id_bytes).to_le_bytes());
            out.extend_from_slice(id_bytes);
            for f in vec {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out
    }

    fn decode(path: &Path, bytes: &[u8]) -> Result<Self, EmbedError> {
        let mut cur = Cursor::new(path, bytes);
        let magic = cur.take(STORE_MAGIC.len())?;
        if magic != STORE_MAGIC {
            return Err(cur.corrupt("bad magic"));
        }
        let version = cur.read_u32()?;
        if version != STORE_VERSION {
            return Err(cur.corrupt(&format!(
                "unsupported version {version} (expected {STORE_VERSION})"
            )));
        }
        let dim = cur.read_u32()?;
        if dim == 0 {
            return Err(cur.corrupt("dim is zero"));
        }
        let model_len = cur.read_u32()? as usize;
        let model_bytes = cur.take(model_len)?;
        let model = std::str::from_utf8(model_bytes)
            .map_err(|_| cur.corrupt("model name is not utf-8"))?
            .to_string();

        let mut vectors = HashMap::new();
        while !cur.is_eof() {
            let id_len = cur.read_u32()? as usize;
            let id_bytes = cur.take(id_len)?;
            let id = std::str::from_utf8(id_bytes)
                .map_err(|_| cur.corrupt("message id is not utf-8"))?
                .to_string();
            let vec_bytes = cur.take((dim as usize) * 4)?;
            let mut vec = Vec::with_capacity(dim as usize);
            for chunk in vec_bytes.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().expect("chunks_exact(4) yields [u8;4]");
                vec.push(f32::from_le_bytes(arr));
            }
            vectors.insert(id, vec);
        }

        Ok(Self {
            path: path.to_path_buf(),
            dim,
            model,
            vectors,
        })
    }
}

fn u32_len(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).expect("string field length fits in u32")
}

struct Cursor<'a> {
    path: &'a Path,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(path: &'a Path, bytes: &'a [u8]) -> Self {
        Self { path, bytes, offset: 0 }
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], EmbedError> {
        if self.offset + n > self.bytes.len() {
            return Err(self.corrupt(&format!(
                "expected {n} bytes at offset {} but only {} remain",
                self.offset,
                self.bytes.len() - self.offset
            )));
        }
        let slice = &self.bytes[self.offset..self.offset + n];
        self.offset += n;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, EmbedError> {
        let bytes = self.take(4)?;
        let arr: [u8; 4] = bytes.try_into().expect("take(4) yields 4 bytes");
        Ok(u32::from_le_bytes(arr))
    }

    fn corrupt(&self, reason: &str) -> EmbedError {
        EmbedError::Corrupt {
            path: self.path.to_path_buf(),
            reason: reason.to_string(),
        }
    }
}

#[cfg(feature = "embeddings")]
pub use feature_gated::Embedder;

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
        /// tokenizer (~256 tokens for MiniLM); we accept that lossiness for v1.
        pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            self.inner
                .embed(texts.to_vec(), None)
                .map_err(|e| EmbedError::Fastembed(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn consent_roundtrip() {
        let dir = tempdir().unwrap();
        assert!(Consent::load(dir.path()).is_none());
        let written = Consent::record(dir.path(), DEFAULT_MODEL).unwrap();
        let loaded = Consent::load(dir.path()).expect("consent should load after record");
        assert_eq!(loaded.model, DEFAULT_MODEL);
        assert_eq!(loaded.accepted_at, written.accepted_at);
    }

    #[test]
    fn store_open_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        assert!(EmbeddingStore::open(dir.path()).unwrap().is_none());
    }

    #[test]
    fn store_roundtrip_preserves_vectors() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        store.upsert("msg-a", vec![0.1, -0.2, 0.3, 0.4]).unwrap();
        store.upsert("msg-b", vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        store.flush().unwrap();

        let loaded = EmbeddingStore::open(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.dim(), 4);
        assert_eq!(loaded.model(), DEFAULT_MODEL);
        assert_eq!(loaded.get("msg-a"), Some([0.1f32, -0.2, 0.3, 0.4].as_slice()));
        assert_eq!(loaded.get("msg-b"), Some([1.0f32, 2.0, 3.0, 4.0].as_slice()));
        assert_eq!(loaded.get("missing"), None);
    }

    #[test]
    fn upsert_rejects_dimension_mismatch() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        let err = store.upsert("msg", vec![0.1, 0.2]).unwrap_err();
        assert!(matches!(
            err,
            EmbedError::DimMismatch { stored: 4, got: 2 }
        ));
    }

    #[test]
    fn corrupt_magic_yields_corrupt_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(STORE_FILENAME);
        fs::write(&path, b"NOTAEMB").unwrap();
        match EmbeddingStore::open(dir.path()) {
            Err(EmbedError::Corrupt { .. }) => {}
            Ok(_) => panic!("expected corrupt error, got Ok"),
            Err(e) => panic!("expected Corrupt, got {e}"),
        }
    }
}
