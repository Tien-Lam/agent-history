use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tantivy::{Index, IndexReader, ReloadPolicy};

mod write;

use crate::fs_atomic;
use crate::fs_read;

use super::fields::SearchFields;
use super::storage::{reset_index_dir, write_index_sentinel};
use super::types::{Manifest, SearchError};

const MAX_SEARCH_MANIFEST_BYTES: usize = 64 * 1024 * 1024;

pub struct SearchIndex {
    pub(super) index: Index,
    pub(super) reader: IndexReader,
    pub(super) fields: SearchFields,
    index_dir: PathBuf,
}

impl SearchIndex {
    pub fn open_or_create(index_dir: &Path) -> Result<Self, SearchError> {
        fs::create_dir_all(index_dir)?;

        let (schema, fields) = SearchFields::build_schema();

        let meta_path = index_dir.join("meta.json");
        // The on-disk index is a cache; if Tantivy can open it but its schema
        // predates fields we now need, rebuild from scratch. A random or
        // corrupt `meta.json` is not treated as our cache and is never reset.
        if meta_path.exists() {
            let existing = Index::open_in_dir(index_dir)?;
            if existing.schema() != schema {
                reset_index_dir(index_dir)?;
            }
        }

        let index = if meta_path.exists() {
            Index::open_in_dir(index_dir)?
        } else {
            Index::create_in_dir(index_dir, schema)?
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        write_index_sentinel(index_dir)?;

        Ok(Self {
            index,
            reader,
            fields,
            index_dir: index_dir.to_path_buf(),
        })
    }

    pub fn num_docs(&self) -> Result<usize, SearchError> {
        self.reader.reload()?;
        Ok(usize::try_from(self.reader.searcher().num_docs()).unwrap_or(usize::MAX))
    }

    pub fn default_index_dir() -> PathBuf {
        default_index_dir_from_env_value(std::env::var_os("AGHIST_INDEX_DIR"))
    }

    fn load_manifest(&self) -> Result<Manifest, SearchError> {
        let path = self.index_dir.join("manifest.json");
        let contents = match fs_read::read_to_string_limited(&path, MAX_SEARCH_MANIFEST_BYTES) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Manifest::default()),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&contents).map_err(Into::into)
    }

    fn save_manifest(&self, manifest: &Manifest) -> Result<(), SearchError> {
        let json = serde_json::to_string(manifest)?;
        fs_atomic::write(&self.index_dir.join("manifest.json"), json.as_bytes())?;
        Ok(())
    }
}

fn default_index_dir_from_env_value(override_dir: Option<OsString>) -> PathBuf {
    if let Some(dir) = override_dir {
        if !os_string_is_blank(&dir) {
            return PathBuf::from(dir);
        }
    }
    default_platform_index_dir()
}

fn os_string_is_blank(value: &OsString) -> bool {
    value.is_empty() || value.to_string_lossy().trim().is_empty()
}

fn default_platform_index_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "aghist").map_or_else(
        || PathBuf::from(".aghist-index"),
        |d| d.cache_dir().join("search-index"),
    )
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn default_index_dir_ignores_empty_env_override() {
        assert_eq!(
            default_index_dir_from_env_value(Some(OsString::new())),
            default_platform_index_dir()
        );
    }

    #[test]
    fn default_index_dir_ignores_blank_env_override() {
        assert_eq!(
            default_index_dir_from_env_value(Some(OsString::from(" \t "))),
            default_platform_index_dir()
        );
    }

    #[test]
    fn default_index_dir_uses_non_empty_env_override() {
        assert_eq!(
            default_index_dir_from_env_value(Some(OsString::from("/tmp/aghist-index"))),
            PathBuf::from("/tmp/aghist-index")
        );
    }
}
