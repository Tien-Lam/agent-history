use std::collections::HashSet;

use tantivy::{IndexWriter, TantivyDocument, Term};

use crate::metadata::Note;
use crate::search::types::{HitKind, NotesIndexStats, SearchError};

use super::super::SearchIndex;

impl SearchIndex {
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
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        let mut manifest = self.load_manifest_or_reset(&mut writer)?;
        let mut stats = NotesIndexStats::default();

        // Track ids seen this pass so we can prune stale manifest entries.
        let mut current_ids: HashSet<String> = HashSet::with_capacity(notes.len());

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
}
