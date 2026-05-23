use std::collections::HashSet;
use std::fs;

use tantivy::{IndexWriter, TantivyDocument, Term};

use crate::model::Provider;

use super::super::types::{Manifest, SearchError};
use super::SearchIndex;

mod notes;
mod sessions;

fn should_prune_session_key(key: &str, prune_providers: Option<&HashSet<Provider>>) -> bool {
    let Some(providers) = prune_providers else {
        return true;
    };
    key.split_once('\x1f')
        .and_then(|(slug, _)| Provider::from_slug(slug))
        .is_some_and(|provider| providers.contains(&provider))
}

impl SearchIndex {
    fn load_manifest_or_reset(
        &self,
        writer: &mut IndexWriter<TantivyDocument>,
    ) -> Result<Manifest, SearchError> {
        match self.load_manifest() {
            Ok(manifest) => Ok(manifest),
            Err(SearchError::Json(_)) => {
                writer.delete_all_documents()?;
                Ok(Manifest::default())
            }
            Err(e) => Err(e),
        }
    }

    pub fn clear(&self) -> Result<(), SearchError> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        writer.commit()?;
        match fs::remove_file(self.index_dir.join("manifest.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    pub fn clear_providers(&self, providers: &HashSet<Provider>) -> Result<(), SearchError> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        let mut manifest = self.load_manifest_or_reset(&mut writer)?;
        for provider in providers {
            writer.delete_term(Term::from_field_text(self.fields.provider, provider.slug()));
        }
        manifest
            .sessions
            .retain(|key, _| !should_prune_session_key(key, Some(providers)));
        writer.commit()?;
        self.save_manifest(&manifest)?;
        Ok(())
    }
}
