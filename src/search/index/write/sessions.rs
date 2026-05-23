use std::collections::HashSet;

use tantivy::{IndexWriter, TantivyDocument, Term};

use crate::action::Action;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;
use crate::search::document::{extract_content, extract_tool_output, message_has_tool_call};
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
        error: error.to_string(),
    });
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

        for (i, session) in sessions.iter().enumerate() {
            let session_key = session.identity_key();
            current_session_keys.insert(session_key.clone());
            let current_fingerprint = match file_fingerprint(&session.source_path) {
                Ok(fingerprint) => fingerprint,
                Err(error) => {
                    record_load_error(&mut stats, session, error);
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
            };

            let change = match manifest.sessions.get(&session_key) {
                Some(cached) if cached == &current_fingerprint => {
                    stats.unchanged += 1;
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
                Some(_) => IndexChange::Updated,
                None => IndexChange::Added,
            };

            let messages = match crate::provider::load_messages_for_session(session, providers) {
                Ok(messages) => messages,
                Err(error) => {
                    record_load_error(&mut stats, session, error);
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
            };
            writer.delete_term(Term::from_field_text(self.fields.session_key, &session_key));
            match change {
                IndexChange::Added => stats.added += 1,
                IndexChange::Updated => stats.updated += 1,
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
                writer.add_document(doc)?;
                stats.messages_indexed += 1;
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
            writer.delete_term(Term::from_field_text(self.fields.session_key, &session_key));
            manifest.sessions.remove(&session_key);
            stats.removed += 1;
        }

        writer.commit()?;
        self.save_manifest(&manifest)?;

        Ok(stats)
    }
}
