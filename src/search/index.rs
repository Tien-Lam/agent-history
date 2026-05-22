use std::fs;
use std::path::{Path, PathBuf};

use tantivy::{Index, IndexReader, ReloadPolicy};

mod write;

use super::fields::SearchFields;
use super::storage::{reset_index_dir, write_index_sentinel};
use super::types::{Manifest, SearchError};

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
        if let Ok(dir) = std::env::var("AGHIST_INDEX_DIR") {
            return PathBuf::from(dir);
        }
        directories::ProjectDirs::from("", "", "aghist").map_or_else(
            || PathBuf::from(".aghist-index"),
            |d| d.cache_dir().join("search-index"),
        )
    }

    fn load_manifest(&self) -> Result<Manifest, SearchError> {
        let path = self.index_dir.join("manifest.json");
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Manifest::default()),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&contents).map_err(Into::into)
    }

    fn save_manifest(&self, manifest: &Manifest) -> Result<(), SearchError> {
        let json = serde_json::to_string(manifest)?;
        fs::write(self.index_dir.join("manifest.json"), json)?;
        Ok(())
    }
}
