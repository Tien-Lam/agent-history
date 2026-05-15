use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::types::{FileFingerprint, Manifest};

pub(super) fn file_fingerprint(path: &Path) -> FileFingerprint {
    if path.is_dir() {
        return dir_fingerprint(path);
    }

    let metadata = path.metadata();
    let len = metadata.as_ref().map_or(0, fs::Metadata::len);
    let modified_nanos = metadata
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    FileFingerprint {
        len,
        modified_nanos,
        sha256: file_sha256(path).unwrap_or_default(),
    }
}

fn dir_fingerprint(path: &Path) -> FileFingerprint {
    let mut files = Vec::new();
    collect_fingerprint_files(path, path, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut len = 0u64;
    let mut modified_nanos = 0u64;
    let mut hasher = Sha256::new();
    for (relative, file) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        if let Ok(metadata) = file.metadata() {
            len = len.saturating_add(metadata.len());
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                    let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
                    modified_nanos = modified_nanos.max(nanos);
                }
            }
        }
        if let Ok(bytes) = fs::read(&file) {
            hasher.update(bytes);
        }
    }

    let digest = hasher.finalize();
    FileFingerprint {
        len,
        modified_nanos,
        sha256: to_hex(&digest),
    }
}

fn collect_fingerprint_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_fingerprint_files(root, &path, out);
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((relative, path));
        }
    }
}

pub(super) fn manifest_has_legacy_path_keys(manifest: &Manifest) -> bool {
    manifest.sessions.keys().any(|key| !key.contains('\x1f'))
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(to_hex(&digest))
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}
