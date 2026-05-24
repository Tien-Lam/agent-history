use std::fs;
use std::path::Path;

use crate::fs_atomic;

use super::types::SearchError;

const INDEX_SENTINEL: &str = ".aghist-search-index";

pub(super) fn write_index_sentinel(index_dir: &Path) -> Result<(), SearchError> {
    fs_atomic::write(&index_dir.join(INDEX_SENTINEL), b"aghist search index\n")?;
    Ok(())
}

pub(super) fn reset_index_dir(index_dir: &Path) -> Result<(), SearchError> {
    require_index_sentinel(index_dir)?;
    let unsafe_entries = unsafe_index_entries(index_dir)?;
    if !unsafe_entries.is_empty() {
        return Err(SearchError::UnsafeIndexDir {
            path: index_dir.to_path_buf(),
            entries: unsafe_entries.join(", "),
        });
    }

    for entry in fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_file() && should_remove_on_index_reset(&path) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

pub fn remove_managed_index_dir(index_dir: &Path) -> Result<bool, SearchError> {
    let metadata = match fs::symlink_metadata(index_dir) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SearchError::UnsafeIndexDir {
            path: index_dir.to_path_buf(),
            entries: "index path is not a directory".to_string(),
        });
    }

    require_index_sentinel(index_dir)?;

    let unsafe_entries = unsafe_index_entries(index_dir)?;
    if !unsafe_entries.is_empty() {
        return Err(SearchError::UnsafeIndexDir {
            path: index_dir.to_path_buf(),
            entries: unsafe_entries.join(", "),
        });
    }

    fs::remove_dir_all(index_dir)?;
    Ok(true)
}

fn require_index_sentinel(index_dir: &Path) -> Result<(), SearchError> {
    let sentinel = index_dir.join(INDEX_SENTINEL);
    match fs::symlink_metadata(&sentinel) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(SearchError::UnsafeIndexDir {
            path: index_dir.to_path_buf(),
            entries: format!("invalid {INDEX_SENTINEL} sentinel"),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(SearchError::UnsafeIndexDir {
            path: index_dir.to_path_buf(),
            entries: format!("missing {INDEX_SENTINEL} sentinel"),
        }),
        Err(e) => Err(e.into()),
    }
}

fn unsafe_index_entries(index_dir: &Path) -> Result<Vec<String>, SearchError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(index_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            if entry.file_name() != "models" {
                entries.push(entry.file_name().to_string_lossy().into_owned());
            }
        } else if file_type.is_symlink() || (file_type.is_file() && !is_known_index_file(&path)) {
            entries.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    entries.sort();
    Ok(entries)
}

fn should_remove_on_index_reset(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_managed_index_file_name)
}

fn is_known_index_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_managed_index_file_name)
}

fn is_managed_index_file_name(name: &str) -> bool {
    name == INDEX_SENTINEL
        || fs_atomic::is_temp_file_for(name, INDEX_SENTINEL)
        || name == "meta.json"
        || name == "manifest.json"
        || fs_atomic::is_temp_file_for(name, "manifest.json")
        || name == "embeddings.bin"
        || name == "embeddings.bin.tmp"
        || fs_atomic::is_temp_file_for(name, "embeddings.bin")
        || name == "embeddings-consent.json"
        || fs_atomic::is_temp_file_for(name, "embeddings-consent.json")
        || is_tantivy_segment_file(name)
}

fn is_tantivy_segment_file(name: &str) -> bool {
    if name == ".managed.json" {
        return true;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() || stem.contains('/') || stem.contains('\\') {
        return false;
    }
    matches!(ext, "idx" | "term" | "store" | "fast" | "fieldnorm" | "pos")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_temp_files_are_managed_index_files() {
        for name in [
            "..aghist-search-index.123-0.tmp",
            ".manifest.json.123-0.tmp",
            ".embeddings.bin.123-0.tmp",
            ".embeddings-consent.json.123-0.tmp",
        ] {
            let path = Path::new(name);
            assert!(is_known_index_file(path), "{name} should be known");
            assert!(
                should_remove_on_index_reset(path),
                "{name} should be resettable"
            );
        }
    }

    #[test]
    fn unrelated_temp_files_are_not_managed_index_files() {
        for name in [
            ".aghist-search-index.123-0.tmp",
            ".notes.json.123-0.tmp",
            "manifest.json.tmp",
            ".manifest.json.tmp",
        ] {
            let path = Path::new(name);
            assert!(!is_known_index_file(path), "{name} should be unknown");
            assert!(
                !should_remove_on_index_reset(path),
                "{name} should not be resettable"
            );
        }
    }

    #[test]
    fn index_reset_requires_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("manifest.json"), "{}").unwrap();

        let err = reset_index_dir(&index_dir).unwrap_err();

        assert!(matches!(err, SearchError::UnsafeIndexDir { .. }));
        assert!(index_dir.join("manifest.json").exists());
    }

    #[test]
    fn index_reset_removes_known_files_when_sentinel_exists() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join(INDEX_SENTINEL), "aghist search index\n").unwrap();
        fs::write(index_dir.join("manifest.json"), "{}").unwrap();
        fs::write(index_dir.join("meta.json"), "{}").unwrap();
        fs::write(index_dir.join("segment.idx"), "index").unwrap();

        reset_index_dir(&index_dir).unwrap();

        assert!(index_dir.exists());
        assert!(!index_dir.join(INDEX_SENTINEL).exists());
        assert!(!index_dir.join("manifest.json").exists());
        assert!(!index_dir.join("meta.json").exists());
        assert!(!index_dir.join("segment.idx").exists());
    }

    #[test]
    fn managed_index_dir_removal_requires_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("manifest.json"), "{}").unwrap();

        let err = remove_managed_index_dir(&index_dir).unwrap_err();

        assert!(matches!(err, SearchError::UnsafeIndexDir { .. }));
        assert!(index_dir.exists());
    }

    #[test]
    fn managed_index_dir_removal_refuses_unknown_entries() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join(INDEX_SENTINEL), "aghist search index\n").unwrap();
        fs::write(index_dir.join("keep.txt"), "user data").unwrap();

        let err = remove_managed_index_dir(&index_dir).unwrap_err();

        assert!(matches!(err, SearchError::UnsafeIndexDir { .. }));
        assert!(index_dir.join("keep.txt").exists());
    }

    #[test]
    fn managed_index_dir_removal_deletes_known_index_tree() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join(INDEX_SENTINEL), "aghist search index\n").unwrap();
        fs::write(index_dir.join("manifest.json"), "{}").unwrap();
        fs::write(index_dir.join("meta.json"), "{}").unwrap();
        fs::create_dir(index_dir.join("models")).unwrap();

        assert!(remove_managed_index_dir(&index_dir).unwrap());

        assert!(!index_dir.exists());
    }

    #[test]
    fn managed_index_dir_removal_ignores_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("missing");

        assert!(!remove_managed_index_dir(&index_dir).unwrap());
    }
}
