use std::collections::HashSet;
use std::fs;

use tempfile::tempdir;

use super::codec::{checked_field_len, MAX_STRING_FIELD_BYTES, STORE_MAGIC, STORE_VERSION};
use super::*;
use crate::embed::{content_hash, DEFAULT_MODEL};

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
fn retain_scoped_keys_preserves_entries_outside_scope() {
    let dir = tempdir().unwrap();
    let mut store = EmbeddingStore::create(dir.path(), DEFAULT_MODEL, 4);
    let hash = content_hash("same");
    store
        .upsert("claude-code\u{1f}keep", hash, vec![1.0, 2.0, 3.0, 4.0])
        .unwrap();
    store
        .upsert("claude-code\u{1f}drop", hash, vec![5.0, 6.0, 7.0, 8.0])
        .unwrap();
    store
        .upsert(
            "copilot-cli\u{1f}outside",
            hash,
            vec![9.0, 10.0, 11.0, 12.0],
        )
        .unwrap();

    let live = HashSet::from(["claude-code\u{1f}keep".to_string()]);

    assert_eq!(
        store.retain_scoped_keys(&live, |key| key.starts_with("claude-code\u{1f}")),
        1
    );
    assert!(store.get("claude-code\u{1f}keep").is_some());
    assert!(store.get("claude-code\u{1f}drop").is_none());
    assert!(store.get("copilot-cli\u{1f}outside").is_some());
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
    // We don't bother filling out the full v1 record body because readers
    // must reject the version before they read records.
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

    // Calling evict on a missing file is a no-op, not an error; callers hit
    // this whenever there's nothing to evict.
    EmbeddingStore::evict(dir.path()).unwrap();
}
