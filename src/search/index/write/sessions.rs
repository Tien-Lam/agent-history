use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tantivy::{IndexWriter, TantivyDocument, Term};

use crate::action::Action;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;
use crate::search::document::{extract_content, extract_tool_output, message_has_tool_call};
use crate::search::fields::SearchFields;
use crate::search::fingerprint::{file_fingerprint, manifest_has_legacy_path_keys};
use crate::search::types::{HitKind, IndexLoadError, IndexStats, Manifest, SearchError};

use super::super::SearchIndex;
use super::should_prune_session_key;

#[derive(Clone, Copy)]
enum IndexChange {
    Added,
    Updated,
}

fn record_load_error(stats: &mut IndexStats, session: &Session, error: impl std::fmt::Display) {
    stats.load_errors.push(IndexLoadError {
        provider: session.provider,
        session_id: session.id.0.clone(),
        session_key: session.identity_key(),
        error: error.to_string(),
    });
}

fn remove_indexed_session(
    writer: &mut IndexWriter<TantivyDocument>,
    manifest: &mut Manifest,
    session_key: &str,
    session_key_field: tantivy::schema::Field,
) {
    writer.delete_term(Term::from_field_text(session_key_field, session_key));
    manifest.sessions.remove(session_key);
}

struct IndexPass<'a> {
    fields: SearchFields,
    writer: &'a mut IndexWriter<TantivyDocument>,
    manifest: &'a mut Manifest,
    stats: &'a mut IndexStats,
    providers: &'a [Box<dyn HistoryProvider>],
    progress_tx: &'a crossbeam_channel::Sender<Action>,
    fingerprint_cache: HashMap<PathBuf, crate::search::types::FileFingerprint>,
    total: usize,
}

impl IndexPass<'_> {
    fn index_session(&mut self, index: usize, session: &Session) -> Result<String, SearchError> {
        let session_key = session.identity_key();
        let current_fingerprint = match self.file_fingerprint(&session.source_path) {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                self.skip_failed_session(index, session, &session_key, error);
                return Ok(session_key);
            }
        };

        let change = match self.manifest.sessions.get(&session_key) {
            Some(cached) if cached == &current_fingerprint => {
                self.stats.unchanged += 1;
                self.report_progress(index);
                return Ok(session_key);
            }
            Some(_) => IndexChange::Updated,
            None => IndexChange::Added,
        };

        let messages = match crate::provider::load_messages_for_session(session, self.providers) {
            Ok(messages) => messages,
            Err(error) => {
                self.skip_failed_session(index, session, &session_key, error);
                return Ok(session_key);
            }
        };

        self.writer
            .delete_term(Term::from_field_text(self.fields.session_key, &session_key));
        match change {
            IndexChange::Added => self.stats.added += 1,
            IndexChange::Updated => self.stats.updated += 1,
        }
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
            self.writer.add_document(doc)?;
            self.stats.messages_indexed += 1;
        }

        self.manifest
            .sessions
            .insert(session_key.clone(), current_fingerprint);
        self.stats.sessions_indexed += 1;
        self.report_progress(index);
        Ok(session_key)
    }

    fn skip_failed_session(
        &mut self,
        index: usize,
        session: &Session,
        session_key: &str,
        error: impl std::fmt::Display,
    ) {
        record_load_error(self.stats, session, error);
        remove_indexed_session(
            self.writer,
            self.manifest,
            session_key,
            self.fields.session_key,
        );
        self.report_progress(index);
    }

    fn report_progress(&self, index: usize) {
        // Progress is best-effort. Non-UI callers may intentionally pass a
        // tiny, undrained channel because they only care about final stats.
        let _ = self
            .progress_tx
            .try_send(Action::IndexProgress(index + 1, self.total));
    }

    fn file_fingerprint(
        &mut self,
        path: &Path,
    ) -> std::io::Result<crate::search::types::FileFingerprint> {
        if let Some(fingerprint) = self.fingerprint_cache.get(path) {
            return Ok(fingerprint.clone());
        }
        let fingerprint = file_fingerprint(path)?;
        self.fingerprint_cache
            .insert(path.to_path_buf(), fingerprint.clone());
        Ok(fingerprint)
    }
}

impl SearchIndex {
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
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        let mut manifest = self.load_manifest_or_reset(&mut writer)?;

        if manifest_has_legacy_path_keys(&manifest) {
            writer.delete_all_documents()?;
            manifest = Manifest::default();
        }

        let total = sessions.len();
        let mut stats = IndexStats::default();
        let mut current_session_keys = HashSet::with_capacity(sessions.len());

        {
            let mut pass = IndexPass {
                fields: self.fields,
                writer: &mut writer,
                manifest: &mut manifest,
                stats: &mut stats,
                providers,
                progress_tx,
                fingerprint_cache: HashMap::new(),
                total,
            };
            for (i, session) in sessions.iter().enumerate() {
                current_session_keys.insert(pass.index_session(i, session)?);
            }
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
            writer.delete_term(Term::from_field_text(self.fields.session_key, &session_key));
            manifest.sessions.remove(&session_key);
            stats.removed += 1;
        }

        writer.commit()?;
        self.save_manifest(&manifest)?;

        Ok(stats)
    }
}
