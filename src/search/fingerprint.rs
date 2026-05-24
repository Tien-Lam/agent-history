use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::types::{FileFingerprint, Manifest};

const MAX_FINGERPRINT_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FINGERPRINT_TREE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FINGERPRINT_TREE_FILES: usize = 100_000;
const MAX_FINGERPRINT_TREE_DIRS: usize = 100_000;

pub(super) fn file_fingerprint(path: &Path) -> io::Result<FileFingerprint> {
    if path.is_dir() {
        return dir_fingerprint(path);
    }

    let metadata = path
        .metadata()
        .map_err(|error| io_with_path("read metadata for", path, &error))?;
    let len = metadata.len();
    checked_file_len(len, path)?;
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
        checked_file_len(metadata.len(), &file)?;
        len = checked_tree_len(len, metadata.len(), path)?;
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
                modified_nanos = modified_nanos.max(nanos);
            }
        }
        hash_file_into(&mut hasher, &file, MAX_FINGERPRINT_FILE_BYTES)?;
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
    collect_fingerprint_files_limited(
        root,
        dir,
        out,
        MAX_FINGERPRINT_TREE_FILES,
        MAX_FINGERPRINT_TREE_DIRS,
    )
}

fn collect_fingerprint_files_limited(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
    max_files: usize,
    max_dirs: usize,
) -> io::Result<()> {
    let mut dirs = vec![dir.to_path_buf()];
    let mut visited_dirs = 0usize;
    while let Some(dir) = dirs.pop() {
        visited_dirs = visited_dirs.saturating_add(1);
        if visited_dirs > max_dirs {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} has too many directories to fingerprint (>{max_dirs})",
                    root.display()
                ),
            ));
        }
        let entries =
            fs::read_dir(&dir).map_err(|error| io_with_path("read directory", &dir, &error))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| io_with_path("read directory entry in", &dir, &error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| io_with_path("read file type for", &entry.path(), &error))?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                dirs.push(path);
            } else if file_type.is_file() {
                if out.len() >= max_files {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "{} has too many files to fingerprint (>{max_files})",
                            root.display()
                        ),
                    ));
                }
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, path));
            }
        }
    }
    Ok(())
}

pub(super) fn manifest_has_legacy_path_keys(manifest: &Manifest) -> bool {
    manifest.sessions.keys().any(|key| !key.contains('\x1f'))
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    hash_file_into(&mut hasher, path, MAX_FINGERPRINT_FILE_BYTES)?;
    let digest = hasher.finalize();
    Ok(to_hex(&digest))
}

fn hash_file_into(hasher: &mut Sha256, path: &Path, max_bytes: u64) -> io::Result<()> {
    let file = fs::File::open(path).map_err(|error| io_with_path("open", path, &error))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let written =
        io::copy(&mut limited, hasher).map_err(|error| io_with_path("read", path, &error))?;
    if written > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is too large to fingerprint (>{} bytes)",
                path.display(),
                max_bytes
            ),
        ));
    }
    Ok(())
}

fn checked_file_len(len: u64, path: &Path) -> io::Result<()> {
    if len > MAX_FINGERPRINT_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is too large to fingerprint (>{} bytes)",
                path.display(),
                MAX_FINGERPRINT_FILE_BYTES
            ),
        ));
    }
    Ok(())
}

fn checked_tree_len(current: u64, next: u64, path: &Path) -> io::Result<u64> {
    let total = current.saturating_add(next);
    if total > MAX_FINGERPRINT_TREE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is too large to fingerprint (>{} bytes total)",
                path.display(),
                MAX_FINGERPRINT_TREE_BYTES
            ),
        ));
    }
    Ok(total)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_fingerprint_collection_rejects_file_count_above_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("one"), "").unwrap();
        std::fs::write(dir.path().join("two"), "").unwrap();
        let mut files = Vec::new();

        let err = collect_fingerprint_files_limited(dir.path(), dir.path(), &mut files, 1, 10)
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too many files"));
    }

    #[test]
    fn directory_fingerprint_collection_rejects_directory_count_above_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        let mut files = Vec::new();

        let err = collect_fingerprint_files_limited(dir.path(), dir.path(), &mut files, 10, 1)
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too many directories"));
    }
}
