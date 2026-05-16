use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::{EmbedError, HASH_LEN};

const STORE_FILENAME: &str = "embeddings.bin";
const STORE_MAGIC: &[u8; 6] = b"AGEMB\0";
/// Bumped from 1 to 2 in ahist-y3o.4.3: each record now carries a 32-byte
/// content hash. Older stores must be evicted (caller deletes the file and
/// builds a fresh one) — readers surface this as `SchemaMismatch`.
const STORE_VERSION: u32 = 2;
const MAX_STRING_FIELD_BYTES: usize = u32::MAX as usize;

/// One stored entry: the content hash that produced this vector, plus the
/// vector itself. Splitting these out makes freshness checks a hash compare
/// without touching the (much larger) `f32` payload.
#[derive(Debug, Clone)]
struct Entry {
    hash: [u8; HASH_LEN],
    vector: Vec<f32>,
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

    /// Atomically rewrite the sidecar with the current contents. Writes to a
    /// `.tmp` file first, then renames — so a crash mid-write can't corrupt
    /// an existing store.
    pub fn flush(&self) -> Result<(), EmbedError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = self.encode()?;
        let tmp = self.path.with_extension("bin.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EmbedError> {
        let per_record = 4 + 32 + HASH_LEN + (self.dim as usize) * 4;
        let mut out = Vec::with_capacity(
            STORE_MAGIC.len() + 4 * 3 + self.model.len() + self.entries.len() * per_record,
        );
        out.extend_from_slice(STORE_MAGIC);
        out.extend_from_slice(&STORE_VERSION.to_le_bytes());
        out.extend_from_slice(&self.dim.to_le_bytes());
        let model_bytes = self.model.as_bytes();
        out.extend_from_slice(&u32_len("model", model_bytes)?.to_le_bytes());
        out.extend_from_slice(model_bytes);
        // Stable order — sorting lets snapshots and fixture tests be deterministic.
        let mut ids: Vec<&String> = self.entries.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &self.entries[id];
            let id_bytes = id.as_bytes();
            out.extend_from_slice(&u32_len("message key", id_bytes)?.to_le_bytes());
            out.extend_from_slice(id_bytes);
            out.extend_from_slice(&entry.hash);
            for f in &entry.vector {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        Ok(out)
    }

    fn decode(path: &Path, bytes: &[u8]) -> Result<Self, EmbedError> {
        let mut cur = Cursor::new(path, bytes);
        let magic = cur.take(STORE_MAGIC.len())?;
        if magic != STORE_MAGIC {
            return Err(cur.corrupt("bad magic"));
        }
        let version = cur.read_u32()?;
        if version != STORE_VERSION {
            // Schema bump — caller is expected to evict and rebuild rather
            // than treat this as corruption.
            return Err(EmbedError::SchemaMismatch {
                stored: version,
                expected: STORE_VERSION,
            });
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

        let mut entries = HashMap::new();
        while !cur.is_eof() {
            let id_len = cur.read_u32()? as usize;
            let id_bytes = cur.take(id_len)?;
            let id = std::str::from_utf8(id_bytes)
                .map_err(|_| cur.corrupt("message id is not utf-8"))?
                .to_string();
            let hash_bytes = cur.take(HASH_LEN)?;
            let hash: [u8; HASH_LEN] = hash_bytes
                .try_into()
                .expect("take(HASH_LEN) yields HASH_LEN bytes");
            let vec_bytes = cur.take((dim as usize) * 4)?;
            let mut vector = Vec::with_capacity(dim as usize);
            for chunk in vec_bytes.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().expect("chunks_exact(4) yields [u8;4]");
                vector.push(f32::from_le_bytes(arr));
            }
            entries.insert(id, Entry { hash, vector });
        }

        Ok(Self {
            path: path.to_path_buf(),
            dim,
            model,
            entries,
        })
    }
}

fn u32_len(field: &'static str, bytes: &[u8]) -> Result<u32, EmbedError> {
    checked_field_len(field, bytes.len())
}

fn checked_field_len(field: &'static str, len: usize) -> Result<u32, EmbedError> {
    u32::try_from(len).map_err(|_| EmbedError::FieldTooLarge {
        field,
        len,
        max: MAX_STRING_FIELD_BYTES,
    })
}

struct Cursor<'a> {
    path: &'a Path,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(path: &'a Path, bytes: &'a [u8]) -> Self {
        Self {
            path,
            bytes,
            offset: 0,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{content_hash, DEFAULT_MODEL};
    use tempfile::tempdir;

    #[test]
    fn store_open_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        assert!(EmbeddingStore::open(dir.path()).unwrap().is_none());
    }

    #[test]
    fn store_roundtrip_preserves_vectors_and_hashes() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        let hash_a = content_hash("hello world");
        let hash_b = content_hash("goodbye world");
        store
            .upsert("msg-a", hash_a, vec![0.1, -0.2, 0.3, 0.4])
            .unwrap();
        store
            .upsert("msg-b", hash_b, vec![1.0, 2.0, 3.0, 4.0])
            .unwrap();
        store.flush().unwrap();

        let loaded = EmbeddingStore::open(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.dim(), 4);
        assert_eq!(loaded.model(), DEFAULT_MODEL);
        assert_eq!(
            loaded.get_if_fresh("msg-a", &hash_a),
            Some([0.1f32, -0.2, 0.3, 0.4].as_slice())
        );
        assert_eq!(
            loaded.get_if_fresh("msg-b", &hash_b),
            Some([1.0f32, 2.0, 3.0, 4.0].as_slice())
        );
        assert_eq!(loaded.get_if_fresh("missing", &hash_a), None);
    }

    #[test]
    fn get_if_fresh_returns_none_when_hash_drifts() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        let original = content_hash("v1 text");
        let updated = content_hash("v2 text");
        store
            .upsert("msg", original, vec![0.1, 0.2, 0.3, 0.4])
            .unwrap();

        // Same id but different content hash: caller should see a miss and
        // re-embed rather than serve a stale vector.
        assert_eq!(store.get_if_fresh("msg", &updated), None);
        // The diagnostic getter still surfaces the old vector — useful for
        // debugging, but not for cache decisions.
        assert!(store.get("msg").is_some());

        // After upserting under the new hash, the cache hits again.
        store
            .upsert("msg", updated, vec![0.5, 0.6, 0.7, 0.8])
            .unwrap();
        assert_eq!(
            store.get_if_fresh("msg", &updated),
            Some([0.5f32, 0.6, 0.7, 0.8].as_slice())
        );
    }

    #[test]
    fn retain_keys_prunes_stale_vectors() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        let hash = content_hash("same");
        store
            .upsert("keep", hash, vec![1.0, 2.0, 3.0, 4.0])
            .unwrap();
        store
            .upsert("drop", hash, vec![5.0, 6.0, 7.0, 8.0])
            .unwrap();

        let live = HashSet::from(["keep".to_string()]);

        assert_eq!(store.retain_keys(&live), 1);
        assert!(store.get("keep").is_some());
        assert!(store.get("drop").is_none());
        assert_eq!(store.retain_keys(&live), 0);
    }

    #[test]
    fn upsert_rejects_dimension_mismatch() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        let err = store
            .upsert("msg", content_hash("x"), vec![0.1, 0.2])
            .unwrap_err();
        assert!(matches!(err, EmbedError::DimMismatch { stored: 4, got: 2 }));
    }

    #[test]
    fn checked_field_len_rejects_values_outside_store_format() {
        assert_eq!(
            checked_field_len("model", MAX_STRING_FIELD_BYTES).unwrap(),
            u32::MAX
        );

        let Some(too_large) = MAX_STRING_FIELD_BYTES.checked_add(1) else {
            return;
        };
        let err = checked_field_len("message key", too_large).unwrap_err();
        assert!(matches!(
            err,
            EmbedError::FieldTooLarge {
                field: "message key",
                len,
                max: MAX_STRING_FIELD_BYTES,
            } if len == too_large
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

    #[test]
    fn old_schema_version_yields_schema_mismatch() {
        // Hand-roll a v1 file (magic + version=1 + minimal trailing bytes).
        // We don't bother filling out the full v1 record body — readers must
        // reject the version before they read records, otherwise they'd misalign.
        let dir = tempdir().unwrap();
        let path = dir.path().join(STORE_FILENAME);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(STORE_MAGIC);
        bytes.extend_from_slice(&1u32.to_le_bytes()); // old version
        bytes.extend_from_slice(&4u32.to_le_bytes()); // dim
        bytes.extend_from_slice(&0u32.to_le_bytes()); // model_len = 0
        fs::write(&path, &bytes).unwrap();

        match EmbeddingStore::open(dir.path()) {
            Err(EmbedError::SchemaMismatch {
                stored: 1,
                expected,
            }) => {
                assert_eq!(expected, STORE_VERSION);
            }
            Ok(_) => panic!("expected SchemaMismatch, got Ok"),
            Err(e) => panic!("expected SchemaMismatch, got {e}"),
        }
    }

    #[test]
    fn evict_removes_existing_sidecar_and_is_idempotent() {
        let dir = tempdir().unwrap();
        let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
        store
            .upsert("msg", content_hash("x"), vec![0.0; 4])
            .unwrap();
        store.flush().unwrap();
        assert!(dir.path().join(STORE_FILENAME).exists());

        EmbeddingStore::evict(dir.path()).unwrap();
        assert!(!dir.path().join(STORE_FILENAME).exists());

        // Calling evict on a missing file is a no-op, not an error — callers
        // hit this whenever there's nothing to evict.
        EmbeddingStore::evict(dir.path()).unwrap();
    }
}
