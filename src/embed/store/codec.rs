use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::embed::{EmbedError, HASH_LEN};

use super::Entry;

pub(super) const STORE_MAGIC: &[u8; 6] = b"AGEMB\0";
/// Bumped from 1 to 2 in ahist-y3o.4.3: each record now carries a 32-byte
/// content hash. Older stores must be evicted (caller deletes the file and
/// builds a fresh one) — readers surface this as `SchemaMismatch`.
pub(super) const STORE_VERSION: u32 = 2;
pub(super) const MAX_STRING_FIELD_BYTES: usize = u32::MAX as usize;
pub(super) const MAX_MODEL_FIELD_BYTES: usize = 4 * 1024;
pub(super) const MAX_MESSAGE_KEY_FIELD_BYTES: usize = 64 * 1024;
pub(super) const MAX_EMBEDDING_DIM: u32 = 8192;

pub(super) struct DecodedStore {
    pub(super) dim: u32,
    pub(super) model: String,
    pub(super) entries: HashMap<String, Entry>,
}

pub(super) fn encode(
    dim: u32,
    model: &str,
    entries: &HashMap<String, Entry>,
) -> Result<Vec<u8>, EmbedError> {
    let dim_usize = checked_dim(dim)?;
    let per_record = 4 + 32 + HASH_LEN + dim_usize * 4;
    let mut out =
        Vec::with_capacity(STORE_MAGIC.len() + 4 * 3 + model.len() + entries.len() * per_record);
    out.extend_from_slice(STORE_MAGIC);
    out.extend_from_slice(&STORE_VERSION.to_le_bytes());
    out.extend_from_slice(&dim.to_le_bytes());
    let model_bytes = model.as_bytes();
    out.extend_from_slice(
        &bounded_u32_len("model", model_bytes, MAX_MODEL_FIELD_BYTES)?.to_le_bytes(),
    );
    out.extend_from_slice(model_bytes);
    // Stable order: sorting lets snapshots and fixture tests be deterministic.
    let mut ids: Vec<&String> = entries.keys().collect();
    ids.sort();
    for id in ids {
        let entry = &entries[id];
        let id_bytes = id.as_bytes();
        out.extend_from_slice(
            &bounded_u32_len("message key", id_bytes, MAX_MESSAGE_KEY_FIELD_BYTES)?.to_le_bytes(),
        );
        out.extend_from_slice(id_bytes);
        out.extend_from_slice(&entry.hash);
        for f in &entry.vector {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    Ok(out)
}

pub(super) fn decode(path: &Path, bytes: &[u8]) -> Result<DecodedStore, EmbedError> {
    let mut cur = Cursor::new(path, bytes);
    let magic = cur.take(STORE_MAGIC.len())?;
    if magic != STORE_MAGIC {
        return Err(cur.corrupt("bad magic"));
    }
    let version = cur.read_u32()?;
    if version != STORE_VERSION {
        // Schema bump: caller is expected to evict and rebuild rather than
        // treat this as corruption.
        return Err(EmbedError::SchemaMismatch {
            stored: version,
            expected: STORE_VERSION,
        });
    }
    let dim = cur.read_u32()?;
    if dim == 0 {
        return Err(cur.corrupt("dim is zero"));
    }
    let dim_usize = cur.checked_dim(dim)?;
    let model_len = cur.read_u32()? as usize;
    cur.check_field_len("model name", model_len, MAX_MODEL_FIELD_BYTES)?;
    let model_bytes = cur.take(model_len)?;
    let model = std::str::from_utf8(model_bytes)
        .map_err(|_| cur.corrupt("model name is not utf-8"))?
        .to_string();

    let mut entries = HashMap::new();
    while !cur.is_eof() {
        let id_len = cur.read_u32()? as usize;
        cur.check_field_len("message id", id_len, MAX_MESSAGE_KEY_FIELD_BYTES)?;
        let id_bytes = cur.take(id_len)?;
        let id = std::str::from_utf8(id_bytes)
            .map_err(|_| cur.corrupt("message id is not utf-8"))?
            .to_string();
        let hash_bytes = cur.take(HASH_LEN)?;
        let hash: [u8; HASH_LEN] = hash_bytes
            .try_into()
            .map_err(|_| cur.corrupt("content hash length mismatch"))?;
        let vec_bytes = cur.take(dim_usize * 4)?;
        let mut vector = Vec::with_capacity(dim_usize);
        for chunk in vec_bytes.chunks_exact(4) {
            let arr: [u8; 4] = chunk
                .try_into()
                .map_err(|_| cur.corrupt("vector chunk length mismatch"))?;
            vector.push(f32::from_le_bytes(arr));
        }
        entries.insert(id, Entry { hash, vector });
    }

    Ok(DecodedStore {
        dim,
        model,
        entries,
    })
}

fn bounded_u32_len(field: &'static str, bytes: &[u8], max: usize) -> Result<u32, EmbedError> {
    if bytes.len() > max {
        return Err(EmbedError::FieldTooLarge {
            field,
            len: bytes.len(),
            max,
        });
    }
    checked_field_len(field, bytes.len())
}

pub(super) fn checked_field_len(field: &'static str, len: usize) -> Result<u32, EmbedError> {
    u32::try_from(len).map_err(|_| EmbedError::FieldTooLarge {
        field,
        len,
        max: MAX_STRING_FIELD_BYTES,
    })
}

fn checked_dim(dim: u32) -> Result<usize, EmbedError> {
    if dim > MAX_EMBEDDING_DIM {
        return Err(EmbedError::FieldTooLarge {
            field: "dimension",
            len: dim as usize,
            max: MAX_EMBEDDING_DIM as usize,
        });
    }
    Ok(dim as usize)
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
        let Some(end) = self.offset.checked_add(n) else {
            return Err(self.corrupt(&format!(
                "field length {n} at offset {} exceeds addressable memory",
                self.offset
            )));
        };
        if end > self.bytes.len() {
            return Err(self.corrupt(&format!(
                "expected {n} bytes at offset {} but only {} remain",
                self.offset,
                self.bytes.len() - self.offset
            )));
        }
        let Some(slice) = self.bytes.get(self.offset..end) else {
            return Err(self.corrupt("field range outside embedding store"));
        };
        self.offset = end;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, EmbedError> {
        let bytes = self.take(4)?;
        let arr: [u8; 4] = bytes
            .try_into()
            .map_err(|_| self.corrupt("u32 field length mismatch"))?;
        Ok(u32::from_le_bytes(arr))
    }

    fn checked_dim(&self, dim: u32) -> Result<usize, EmbedError> {
        if dim > MAX_EMBEDDING_DIM {
            return Err(self.corrupt(&format!(
                "dim {dim} exceeds max supported dimension {MAX_EMBEDDING_DIM}"
            )));
        }
        Ok(dim as usize)
    }

    fn check_field_len(
        &self,
        field: &'static str,
        len: usize,
        max: usize,
    ) -> Result<(), EmbedError> {
        if len > max {
            return Err(self.corrupt(&format!(
                "{field} length {len} exceeds max supported length {max}"
            )));
        }
        Ok(())
    }

    fn corrupt(&self, reason: &str) -> EmbedError {
        EmbedError::Corrupt {
            path: PathBuf::from(self.path),
            reason: reason.to_string(),
        }
    }
}
