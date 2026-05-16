use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::action::Action;
use crate::metadata::Note;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;

use super::document::{extract_content, extract_tool_output, message_has_tool_call};
use super::fields::SearchFields;
use super::fingerprint::{file_fingerprint, manifest_has_legacy_path_keys};
use super::storage::{reset_index_dir, write_index_sentinel};
use super::types::{HitKind, IndexStats, Manifest, NotesIndexStats, SearchError};

fn should_prune_session_key(key: &str, prune_providers: Option<&HashSet<Provider>>) -> bool {
    let Some(providers) = prune_providers else {
        return true;
    };
    key.split_once('\x1f')
        .and_then(|(slug, _)| Provider::from_slug(slug))
        .is_some_and(|provider| providers.contains(&provider))
}

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

    pub fn build_index(
        &self,
        sessions: &[Session],
        providers: &[Box<dyn HistoryProvider>],
        progress_tx: &crossbeam_channel::Sender<Action>,
    ) -> Result<IndexStats, SearchError> {
        self.build_index_inner(sessions, providers, progress_tx, None)
    }

    pub fn build_index_for_providers(
        &self,
        sessions: &[Session],
        providers: &[Box<dyn HistoryProvider>],
        progress_tx: &crossbeam_channel::Sender<Action>,
        prune_providers: &HashSet<Provider>,
    ) -> Result<IndexStats, SearchError> {
        self.build_index_inner(sessions, providers, progress_tx, Some(prune_providers))
    }

    pub fn build_index_without_pruning(
        &self,
        sessions: &[Session],
        providers: &[Box<dyn HistoryProvider>],
        progress_tx: &crossbeam_channel::Sender<Action>,
    ) -> Result<IndexStats, SearchError> {
        let prune_providers = HashSet::new();
        self.build_index_inner(sessions, providers, progress_tx, Some(&prune_providers))
    }

    fn build_index_inner(
        &self,
        sessions: &[Session],
        providers: &[Box<dyn HistoryProvider>],
        progress_tx: &crossbeam_channel::Sender<Action>,
        prune_providers: Option<&HashSet<Provider>>,
    ) -> Result<IndexStats, SearchError> {
        let mut manifest = self.load_manifest();
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;

        if manifest_has_legacy_path_keys(&manifest) {
            writer.delete_all_documents()?;
            manifest = Manifest::default();
        }

        let total = sessions.len();
        let mut stats = IndexStats::default();
        let mut current_session_keys = HashSet::with_capacity(sessions.len());

        for (i, session) in sessions.iter().enumerate() {
            let current_fingerprint = file_fingerprint(&session.source_path);
            let session_key = session.identity_key();
            current_session_keys.insert(session_key.clone());

            let existing_fingerprint = manifest.sessions.get(&session_key);
            match existing_fingerprint {
                Some(cached) if cached == &current_fingerprint => {
                    stats.unchanged += 1;
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
                Some(_) => stats.updated += 1,
                None => stats.added += 1,
            }

            writer.delete_term(tantivy::Term::from_field_text(
                self.fields.session_key,
                &session_key,
            ));

            if let Ok(messages) = crate::provider::load_messages_for_session(session, providers) {
                for (turn_index, msg) in messages.iter().enumerate() {
                    let content = extract_content(msg);
                    let tool_output = extract_tool_output(msg);
                    if content.is_empty() && tool_output.is_empty() {
                        continue;
                    }
                    let project = session.project_name.as_deref().unwrap_or("");
                    let has_tool_call = i64::from(message_has_tool_call(msg));
                    let message_key = session.message_key(turn_index, &msg.id.0);
                    let mut doc = TantivyDocument::default();
                    doc.add_text(self.fields.kind, HitKind::Message.slug());
                    doc.add_text(self.fields.session_key, &session_key);
                    doc.add_text(self.fields.session_id, &session.id.0);
                    doc.add_text(self.fields.message_key, &message_key);
                    doc.add_text(self.fields.message_id, &msg.id.0);
                    doc.add_text(self.fields.provider, session.provider.slug());
                    doc.add_text(self.fields.project, project);
                    doc.add_text(self.fields.project_raw, project);
                    doc.add_text(self.fields.role, msg.role.slug());
                    doc.add_text(self.fields.content, &content);
                    doc.add_text(self.fields.tool_output, &tool_output);
                    doc.add_i64(self.fields.timestamp, msg.timestamp.timestamp());
                    doc.add_i64(self.fields.has_tool_call, has_tool_call);
                    writer.add_document(doc)?;
                    stats.messages_indexed += 1;
                }
            } else {
                let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                continue;
            }

            manifest.sessions.insert(session_key, current_fingerprint);
            stats.sessions_indexed += 1;
            let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
        }

        let stale: Vec<String> = manifest
            .sessions
            .keys()
            .filter(|key| {
                !current_session_keys.contains(*key)
                    && should_prune_session_key(key, prune_providers)
            })
            .cloned()
            .collect();
        for session_key in stale {
            writer.delete_term(tantivy::Term::from_field_text(
                self.fields.session_key,
                &session_key,
            ));
            manifest.sessions.remove(&session_key);
            stats.removed += 1;
        }

        writer.commit()?;
        self.save_manifest(&manifest)?;

        Ok(stats)
    }

    /// Index notes from the metadata sidecar so they show up alongside session
    /// content in `search`. Incremental: a note whose `updated_at` matches the
    /// manifest snapshot is skipped, so repeat calls are cheap. Notes present
    /// in the manifest but absent from `notes` are removed from the index, so
    /// metadata.db deletes propagate.
    ///
    /// Each indexed note becomes a Tantivy doc with `kind="note"`, the note id
    /// in `note_id`, the `session_ref` in `note_session_ref`, and the body in
    /// `content` (so the same query parser that searches messages also matches
    /// notes). Note docs intentionally omit provider/role/timestamp fields:
    /// notes don't belong to a single message turn, so any `--provider`,
    /// `--role`, or `--since/--until` filter at search time will exclude them
    /// via Tantivy's MUST clauses — which is the right behaviour, since those
    /// dimensions don't apply to a free-form annotation.
    pub fn index_notes(&self, notes: &[Note]) -> Result<NotesIndexStats, SearchError> {
        let mut manifest = self.load_manifest();
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        let mut stats = NotesIndexStats::default();

        // Track ids seen this pass so we can prune stale manifest entries.
        let mut current_ids: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(notes.len());

        for note in notes {
            let key = note.id.to_string();
            current_ids.insert(key.clone());

            match manifest.notes.get(&key) {
                Some(prev) if prev == &note.updated_at => {
                    stats.unchanged += 1;
                    continue;
                }
                Some(_) => stats.updated += 1,
                None => stats.added += 1,
            }

            // delete-by-term keys on the i64 note_id field — message docs
            // don't carry note_id so they're untouched.
            writer.delete_term(Term::from_field_i64(self.fields.note_id, note.id));

            let mut doc = TantivyDocument::default();
            doc.add_text(self.fields.kind, HitKind::Note.slug());
            doc.add_i64(self.fields.note_id, note.id);
            doc.add_text(self.fields.note_session_ref, &note.session_ref);
            doc.add_text(self.fields.content, &note.body);
            writer.add_document(doc)?;

            manifest.notes.insert(key, note.updated_at.clone());
        }

        // Prune notes that vanished from the sidecar.
        let stale: Vec<(String, i64)> = manifest
            .notes
            .keys()
            .filter(|k| !current_ids.contains(*k))
            .filter_map(|k| k.parse::<i64>().ok().map(|id| (k.clone(), id)))
            .collect();
        for (key, id) in stale {
            writer.delete_term(Term::from_field_i64(self.fields.note_id, id));
            manifest.notes.remove(&key);
            stats.removed += 1;
        }

        writer.commit()?;
        self.save_manifest(&manifest)?;
        Ok(stats)
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
        for provider in providers {
            writer.delete_term(Term::from_field_text(self.fields.provider, provider.slug()));
        }
        writer.commit()?;

        let mut manifest = self.load_manifest();
        manifest
            .sessions
            .retain(|key, _| !should_prune_session_key(key, Some(providers)));
        self.save_manifest(&manifest)?;
        Ok(())
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

    fn load_manifest(&self) -> Manifest {
        let path = self.index_dir.join("manifest.json");
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_manifest(&self, manifest: &Manifest) -> Result<(), SearchError> {
        let json = serde_json::to_string(manifest)?;
        fs::write(self.index_dir.join("manifest.json"), json)?;
        Ok(())
    }
}
