use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{fs, io};

use sha2::{Digest, Sha256};

use super::types::{FileFingerprint, Manifest};

pub(super) fn file_fingerprint(path: &Path) -> io::Result<FileFingerprint> {
    if path.is_dir() {
        return dir_fingerprint(path);
    }

    let metadata = path
        .metadata()
        .map_err(|error| io_with_path("read metadata for", path, &error))?;
    let len = metadata.len();
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    Ok(FileFingerprint {
        len,
        modified_nanos,
        sha256: file_sha256(path)?,
    })
}

fn dir_fingerprint(path: &Path) -> io::Result<FileFingerprint> {
    let mut files = Vec::new();
    collect_fingerprint_files(path, path, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut len = 0u64;
    let mut modified_nanos = 0u64;
    let mut hasher = Sha256::new();
    for (relative, file) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let metadata = file
            .metadata()
            .map_err(|error| io_with_path("read metadata for", &file, &error))?;
        len = len.saturating_add(metadata.len());
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
                modified_nanos = modified_nanos.max(nanos);
            }
        }
        let bytes = fs::read(&file).map_err(|error| io_with_path("read", &file, &error))?;
        hasher.update(bytes);
    }

    let digest = hasher.finalize();
    Ok(FileFingerprint {
        len,
        modified_nanos,
        sha256: to_hex(&digest),
    })
}

fn collect_fingerprint_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> io::Result<()> {
    let entries = fs::read_dir(dir).map_err(|error| io_with_path("read directory", dir, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_with_path("read directory entry in", dir, &error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| io_with_path("read file type for", &entry.path(), &error))?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_fingerprint_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((relative, path));
        }
    }
    Ok(())
}

pub(super) fn manifest_has_legacy_path_keys(manifest: &Manifest) -> bool {
    manifest.sessions.keys().any(|key| !key.contains('\x1f'))
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path).map_err(|error| io_with_path("read", path, &error))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(to_hex(&digest))
}

fn io_with_path(action: &str, path: &Path, error: &io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{action} {}: {error}", path.display()),
    )
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(hex_char(b >> 4));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(nibble: u8) -> char {
    char::from_digit(u32::from(nibble), 16).unwrap_or('0')
}
