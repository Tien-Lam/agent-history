use std::fs;
use std::path::Path;

use crate::fs_atomic;

use super::types::SearchError;

const INDEX_SENTINEL: &str = ".aghist-search-index";

pub(super) fn write_index_sentinel(index_dir: &Path) -> Result<(), SearchError> {
    fs::write(index_dir.join(INDEX_SENTINEL), b"aghist search index\n")?;
    Ok(())
}

pub(super) fn reset_index_dir(index_dir: &Path) -> Result<(), SearchError> {
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
