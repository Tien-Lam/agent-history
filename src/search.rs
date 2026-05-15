use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Bound;
use std::path::{Path, PathBuf};

use tantivy::collector::{DocSetCollector, TopDocs};
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

pub use tantivy::query::Explanation;

use crate::action::Action;
use crate::metadata::Note;
use crate::model::{Role, Session};
use crate::provider::HistoryProvider;

mod document;
mod fields;
mod fingerprint;
mod snippet;
mod storage;
pub mod types;
use document::{
    extract_content, extract_tool_output, field_i64, field_text, message_has_tool_call,
};
use fields::SearchFields;
use fingerprint::{file_fingerprint, manifest_has_legacy_path_keys};
use snippet::best_snippet;
use storage::{reset_index_dir, write_index_sentinel};
use types::Manifest;
pub use types::{
    HitKind, IndexStats, NotesIndexStats, SearchError, SearchFilters, SearchHit, SemanticCandidate,
    RRF_K,
};

/// Cosine similarity in [-1, 1]. Returns 0.0 for mismatched / empty / zero-norm
/// vectors — those can't yield a useful hybrid signal anyway.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    fields: SearchFields,
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
            .filter(|key| !current_session_keys.contains(*key))
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

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>, SearchError> {
        self.search_with_filters(query_str, limit, &SearchFilters::default())
    }

    /// Search with structured filters applied alongside the user query.
    ///
    /// Provider/role/timestamp/has-tool-call filters are pushed into Tantivy as
    /// boolean MUST clauses (cheap, scaled by the index). The project filter is
    /// applied as a post-filter substring match against the stored project
    /// value, since project names can contain arbitrary characters that don't
    /// round-trip cleanly through the analyzed `project` text field.
    pub fn search_with_filters(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHit>, SearchError> {
        Ok(self
            .search_inner(query_str, limit, filters, false)?
            .into_iter()
            .map(|(hit, _)| hit)
            .collect())
    }

    /// Like [`Self::search_with_filters`] but also returns Tantivy's BM25
    /// [`Explanation`] tree for each hit, so callers can surface a score
    /// breakdown (the `--debug-search` flag).
    pub fn search_with_filters_and_explain(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<(SearchHit, Explanation)>, SearchError> {
        let raw = self.search_inner(query_str, limit, filters, true)?;
        Ok(raw
            .into_iter()
            .map(|(hit, explain)| {
                // explain=true guarantees Some; fall back to a stub if Tantivy
                // ever returns a hit without an explainable scorer.
                let explanation = explain.unwrap_or_else(|| {
                    Explanation::new_with_string("no explanation available".into(), hit.score)
                });
                (hit, explanation)
            })
            .collect())
    }

    /// Hybrid search blending BM25 lexical ranking with caller-provided
    /// semantic candidates via Reciprocal Rank Fusion.
    ///
    /// `semantic_ranked` is the top-N from cosine similarity over the
    /// embedding store, already sorted by similarity DESC. The caller does the
    /// embedding/ranking; this method only fuses ranks. That keeps the search
    /// crate ignorant of `fastembed` so lean (no-feature) builds still link.
    ///
    /// `hybrid_weight` is the RRF weight on the semantic side, clamped to
    /// `[0.0, 1.0]`:
    /// - `0.0` → equivalent to [`Self::search_with_filters`] (lexical only).
    /// - `1.0` → semantic only; lexical pool only contributes if it overlaps.
    /// - `0.5` → equal RRF blend.
    ///
    /// Both pools are filter-validated: lexical via Tantivy MUST clauses on
    /// the BM25 query; semantic via a separate Tantivy lookup that re-applies
    /// the same filters to each semantic candidate. Callers can therefore
    /// rely on `filters` having the same meaning whether hybrid is on or off.
    ///
    /// `candidate_pool` caps the per-side pool. Larger pools surface more
    /// "semantic only" hits at the cost of more Tantivy lookups; defaults to
    /// `max(limit, 50)` are reasonable.
    ///
    /// Returns `Vec<SearchHit>` whose `score` is the RRF fused score (small,
    /// roughly `[0, 2/(K+1)]`) — not directly comparable to BM25 scores from
    /// the lexical-only path.
    pub fn search_hybrid(
        &self,
        query_str: &str,
        semantic_ranked: &[SemanticCandidate],
        limit: usize,
        filters: &SearchFilters,
        hybrid_weight: f32,
        candidate_pool: usize,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let weight = hybrid_weight.clamp(0.0, 1.0);

        // Fail-open paths: weight==0 or no semantic input means "no useful
        // semantic signal", so behave exactly like the lexical-only call.
        // We intentionally do NOT short-circuit on weight==1.0; the lexical
        // pool is still useful for filter coverage when semantic misses.
        if weight == 0.0 || semantic_ranked.is_empty() {
            return self.search_with_filters(query_str, limit, filters);
        }

        let pool = candidate_pool.max(limit).max(1);

        let lexical: Vec<SearchHit> = self
            .search_inner(query_str, pool, filters, false)?
            .into_iter()
            .map(|(h, _)| h)
            .collect();

        let sem_keys: Vec<&str> = semantic_ranked
            .iter()
            .take(pool)
            .map(|c| c.message_key.as_str())
            .collect();
        let semantic: Vec<SearchHit> =
            self.fetch_filtered_by_message_keys(query_str, &sem_keys, filters)?;

        // Tuple is (lexical rank 1-based, semantic rank 1-based, hit). A None
        // rank means "absent from that pool" → that side contributes 0 to RRF.
        let mut by_id: HashMap<String, (Option<usize>, Option<usize>, SearchHit)> = HashMap::new();
        for (i, hit) in lexical.into_iter().enumerate() {
            by_id.insert(hit.message_key.clone(), (Some(i + 1), None, hit));
        }
        for (i, hit) in semantic.into_iter().enumerate() {
            by_id
                .entry(hit.message_key.clone())
                .and_modify(|entry| entry.1 = Some(i + 1))
                .or_insert((None, Some(i + 1), hit));
        }

        let w_sem = weight;
        let w_lex = 1.0 - weight;
        let mut fused: Vec<SearchHit> = by_id
            .into_values()
            .map(|(lex_rank, sem_rank, mut hit)| {
                #[allow(clippy::cast_precision_loss)]
                let lex_term = lex_rank.map_or(0.0, |r| w_lex / (RRF_K + r as f32));
                #[allow(clippy::cast_precision_loss)]
                let sem_term = sem_rank.map_or(0.0, |r| w_sem / (RRF_K + r as f32));
                hit.score = lex_term + sem_term;
                hit
            })
            .collect();

        // Score DESC; deterministic tie-break by (session_id ASC, message_id
        // ASC) so test snapshots and pagination cursors stay stable.
        fused.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.session_key.cmp(&b.session_key))
                .then_with(|| a.message_key.cmp(&b.message_key))
        });
        fused.truncate(limit);
        Ok(fused)
    }

    /// Look up indexed messages by internal `message_key`, applying the same filter
    /// clauses as [`Self::search_inner`]. Used by hybrid search to validate
    /// semantic candidates against server-side filters before fusing.
    ///
    /// The returned vector preserves the input order (typically similarity
    /// DESC), with non-matching ids dropped — so callers can use position as
    /// the semantic rank.
    fn fetch_filtered_by_message_keys(
        &self,
        query_str: &str,
        keys: &[&str],
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHit>, SearchError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        // OR of message_id terms — Tantivy doesn't have an IN query, so we
        // build a Should-clause boolean.
        let id_clauses: Vec<(Occur, Box<dyn Query>)> = keys
            .iter()
            .map(|key| {
                let term = Term::from_field_text(self.fields.message_key, key);
                (
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as Box<dyn Query>,
                )
            })
            .collect();
        let id_query: Box<dyn Query> = Box::new(BooleanQuery::new(id_clauses));

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(5);
        clauses.push((Occur::Must, id_query));
        self.add_filter_clauses(&mut clauses, filters);

        let combined: Box<dyn Query> = Box::new(BooleanQuery::new(clauses));

        // Cap at the input length — we have at most one hit per requested key.
        let top_docs =
            searcher.search(&combined, &TopDocs::with_limit(keys.len()).order_by_score())?;

        let project_needle = Self::project_filter_needle(filters);

        let mut by_msg_key: HashMap<String, SearchHit> = HashMap::new();
        for (_score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if !self.matches_project_filter(&doc, project_needle.as_deref()) {
                continue;
            }
            let session_key = field_text(&doc, self.fields.session_key);
            let session_id = field_text(&doc, self.fields.session_id);
            let message_key = field_text(&doc, self.fields.message_key);
            let message_id = field_text(&doc, self.fields.message_id);
            let content = field_text(&doc, self.fields.content);
            let tool_output = field_text(&doc, self.fields.tool_output);
            let snippet = best_snippet(&content, &tool_output, query_str, 120);
            by_msg_key.insert(
                message_key.clone(),
                SearchHit {
                    kind: HitKind::Message,
                    session_key,
                    session_id,
                    message_key,
                    message_id,
                    snippet,
                    score: 0.0,
                    note_id: None,
                    note_session_ref: None,
                },
            );
        }

        // Drain in input order. Matches positionally to similarity rank.
        Ok(keys
            .iter()
            .filter_map(|key| by_msg_key.remove(*key))
            .collect())
    }

    fn add_filter_clauses(
        &self,
        clauses: &mut Vec<(Occur, Box<dyn Query>)>,
        filters: &SearchFilters,
    ) {
        if let Some(provider) = filters.provider {
            let term = Term::from_field_text(self.fields.provider, provider.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if let Some(role) = filters.role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.since.is_some() || filters.until.is_some() {
            let lower = filters.since.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            let upper = filters.until.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }
    }

    fn project_filter_needle(filters: &SearchFilters) -> Option<String> {
        filters
            .project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty())
    }

    fn matches_project_filter(&self, doc: &TantivyDocument, needle: Option<&str>) -> bool {
        let Some(needle) = needle else {
            return true;
        };
        field_text(doc, self.fields.project_raw)
            .to_lowercase()
            .contains(needle)
    }

    fn search_inner(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
        explain: bool,
    ) -> Result<Vec<(SearchHit, Option<Explanation>)>, SearchError> {
        if query_str.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.content,
                self.fields.project,
                self.fields.tool_output,
            ],
        );
        let user_query = parser.parse_query(query_str)?;

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(6);
        clauses.push((Occur::Must, user_query));
        self.add_filter_clauses(&mut clauses, filters);

        let combined: Box<dyn Query> = if clauses.len() == 1 {
            clauses.into_iter().next().expect("one clause").1
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        // Project is post-filtered; over-fetch to keep results stable when a
        // restrictive project filter would otherwise prune the limit-N window.
        let project_needle = Self::project_filter_needle(filters);
        let fetch_limit = if project_needle.is_some() {
            limit.saturating_mul(8).max(limit)
        } else {
            limit
        };

        let top_docs = searcher.search(
            &combined,
            &TopDocs::with_limit(fetch_limit).order_by_score(),
        )?;

        let mut hits = Vec::with_capacity(top_docs.len().min(limit));
        for (score, addr) in top_docs {
            if hits.len() >= limit {
                break;
            }
            let doc: TantivyDocument = searcher.doc(addr)?;
            if !self.matches_project_filter(&doc, project_needle.as_deref()) {
                continue;
            }
            let hit = self.doc_to_hit(&doc, query_str, score);
            let explanation = if explain {
                Some(combined.explain(&searcher, addr)?)
            } else {
                None
            };
            hits.push((hit, explanation));
        }

        Ok(hits)
    }

    /// Return the set of session IDs that contain at least one indexed message
    /// matching the given message-level filters. Used by the TUI filter panel
    /// to live-filter the session list by role / has-tool-call without
    /// loading every session's messages into memory.
    ///
    /// Returns an empty set when no filter is active (caller should treat
    /// "no filter" as "no constraint", not "show nothing").
    pub fn session_ids_with_messages(
        &self,
        role: Option<Role>,
        has_tool_call: bool,
    ) -> Result<HashSet<String>, SearchError> {
        if role.is_none() && !has_tool_call {
            return Ok(HashSet::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        if let Some(role) = role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        let combined: Box<dyn Query> = if clauses.len() == 1 {
            clauses.into_iter().next().expect("one clause").1
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        // Walk every matching message and collect distinct internal session keys.
        // Ranking is irrelevant here — `DocSetCollector` is cheaper than
        // `TopDocs` because it skips score tracking and has no top-N cap.
        let docs = searcher.search(&combined, &DocSetCollector)?;

        let mut session_ids = HashSet::new();
        for addr in docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            session_ids.insert(field_text(&doc, self.fields.session_key));
        }
        Ok(session_ids)
    }

    pub fn clear(&self) -> Result<(), SearchError> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        writer.commit()?;
        let _ = fs::remove_file(self.index_dir.join("manifest.json"));
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

    /// Materialize a stored Tantivy doc into a [`SearchHit`], reading the
    /// `kind` field to drive whether note metadata is populated. Centralized
    /// so message-only and hybrid paths both produce a uniformly-shaped hit.
    fn doc_to_hit(&self, doc: &TantivyDocument, query_str: &str, score: f32) -> SearchHit {
        let kind = if field_text(doc, self.fields.kind) == HitKind::Note.slug() {
            HitKind::Note
        } else {
            HitKind::Message
        };
        let session_key = field_text(doc, self.fields.session_key);
        let session_id = field_text(doc, self.fields.session_id);
        let mut message_key = field_text(doc, self.fields.message_key);
        let message_id = field_text(doc, self.fields.message_id);
        let content = field_text(doc, self.fields.content);
        let tool_output = field_text(doc, self.fields.tool_output);
        let snippet = best_snippet(&content, &tool_output, query_str, 120);
        let (note_id, note_session_ref) = match kind {
            HitKind::Note => {
                let r = field_text(doc, self.fields.note_session_ref);
                let r = if r.is_empty() { None } else { Some(r) };
                let id = field_i64(doc, self.fields.note_id);
                if let Some(id) = id {
                    message_key = format!("note:{id}");
                }
                (id, r)
            }
            HitKind::Message => (None, None),
        };
        SearchHit {
            kind,
            session_key,
            session_id,
            message_key,
            message_id,
            snippet,
            score,
            note_id,
            note_session_ref,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, Message, MessageId, Provider, Role, SessionId};
    use chrono::TimeZone;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::tempdir;

    /// A `HistoryProvider` that hands back a canned message list per session.
    /// Lets unit tests build a Tantivy index without going through real disk
    /// fixtures.
    struct StubProvider {
        provider: Provider,
        // Mutex so the trait's `&self` sig can hand out cloned messages without
        // the caller noticing — we don't actually share across threads in tests.
        sessions: Mutex<Vec<Session>>,
        messages: Mutex<std::collections::HashMap<String, Vec<Message>>>,
        base: Vec<PathBuf>,
    }

    impl StubProvider {
        fn new(provider: Provider) -> Self {
            Self {
                provider,
                sessions: Mutex::new(Vec::new()),
                messages: Mutex::new(std::collections::HashMap::new()),
                base: Vec::new(),
            }
        }

        fn add(&self, session: Session, messages: Vec<Message>) {
            self.messages
                .lock()
                .unwrap()
                .insert(session.identity_key(), messages);
            self.sessions.lock().unwrap().push(session);
        }
    }

    impl crate::provider::HistoryProvider for StubProvider {
        fn provider(&self) -> Provider {
            self.provider
        }
        fn base_dirs(&self) -> &[PathBuf] {
            &self.base
        }
        fn discover_sessions(&self) -> Result<Vec<Session>, crate::provider::ProviderError> {
            Ok(self.sessions.lock().unwrap().clone())
        }
        fn load_messages(
            &self,
            session: &Session,
        ) -> Result<Vec<Message>, crate::provider::ProviderError> {
            Ok(self
                .messages
                .lock()
                .unwrap()
                .get(&session.identity_key())
                .cloned()
                .unwrap_or_default())
        }
    }

    fn make_session(id: &str, project: &str) -> Session {
        Session {
            id: SessionId(id.to_string()),
            provider: Provider::ClaudeCode,
            project_path: Some(PathBuf::from(format!("/proj/{project}"))),
            project_name: Some(project.to_string()),
            git_branch: None,
            started_at: chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 0,
            // source_path mtime is read for the manifest; using a real (but
            // empty) tempfile path keeps `file_mtime` happy without crashing.
            source_path: PathBuf::from(format!("/tmp/aghist-stub-{id}")),
        }
    }

    fn make_message(id: &str, text: &str) -> Message {
        Message {
            id: MessageId(id.to_string()),
            role: Role::User,
            timestamp: chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            content: vec![ContentBlock::Text(text.to_string())],
            model: None,
            token_usage: None,
        }
    }

    /// Builds a tiny Tantivy index with two sessions / four messages and
    /// returns the open `SearchIndex`. Used to exercise hybrid scoring against
    /// a real index without depending on filesystem fixtures.
    fn build_tiny_index() -> (tempfile::TempDir, SearchIndex) {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();

        let stub = StubProvider::new(Provider::ClaudeCode);
        let s1 = make_session("sess-1", "alpha");
        let s2 = make_session("sess-2", "beta");
        stub.add(
            s1.clone(),
            vec![
                make_message("m-1", "rust async tokio runtime executor"),
                make_message("m-2", "ratatui terminal user interface"),
            ],
        );
        stub.add(
            s2.clone(),
            vec![
                make_message("m-3", "tantivy full text search engine"),
                make_message("m-4", "fastembed semantic vectors"),
            ],
        );

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let sessions: Vec<Session> = vec![s1, s2];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index.build_index(&sessions, &providers, &tx).unwrap();

        (dir, index)
    }

    #[test]
    fn duplicate_raw_session_ids_do_not_overwrite_each_other() {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();
        let stub = StubProvider::new(Provider::ClaudeCode);

        let mut first = make_session("shared-id", "alpha");
        first.source_path = dir.path().join("first.jsonl");
        std::fs::write(&first.source_path, "first").unwrap();
        let mut second = make_session("shared-id", "beta");
        second.source_path = dir.path().join("second.jsonl");
        std::fs::write(&second.source_path, "second").unwrap();

        stub.add(
            first.clone(),
            vec![make_message("msg", "alpha unique overwrite guard")],
        );
        stub.add(
            second.clone(),
            vec![make_message("msg", "beta unique overwrite guard")],
        );

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let sessions = vec![first, second];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index.build_index(&sessions, &providers, &tx).unwrap();

        let alpha = index.search("alpha", 10).unwrap();
        let beta = index.search("beta", 10).unwrap();
        assert_eq!(
            alpha.len(),
            1,
            "first duplicate-id session was lost: {alpha:?}"
        );
        assert_eq!(
            beta.len(),
            1,
            "second duplicate-id session was lost: {beta:?}"
        );
        assert_eq!(alpha[0].session_id, "shared-id");
        assert_eq!(beta[0].session_id, "shared-id");
        assert_ne!(alpha[0].session_key, beta[0].session_key);
        assert_ne!(alpha[0].message_key, beta[0].message_key);
    }

    #[test]
    fn sessions_sharing_one_source_path_are_all_indexed() {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();
        let stub = StubProvider::new(Provider::Cursor);

        let shared_db = dir.path().join("state.vscdb");
        std::fs::write(&shared_db, "cursor db snapshot").unwrap();
        let mut first = make_session("composer-a", "alpha");
        first.provider = Provider::Cursor;
        first.source_path = shared_db.clone();
        let mut second = make_session("composer-b", "beta");
        second.provider = Provider::Cursor;
        second.source_path = shared_db;

        stub.add(
            first.clone(),
            vec![make_message("msg-a", "alpha cursor composer")],
        );
        stub.add(
            second.clone(),
            vec![make_message("msg-b", "beta cursor composer")],
        );

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let sessions = vec![first, second];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index.build_index(&sessions, &providers, &tx).unwrap();

        assert_eq!(index.search("alpha", 10).unwrap().len(), 1);
        assert_eq!(index.search("beta", 10).unwrap().len(), 1);
    }

    #[test]
    fn build_index_prunes_sessions_no_longer_discovered() {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();
        let stub = StubProvider::new(Provider::ClaudeCode);

        let mut first = make_session("sess-a", "alpha");
        first.source_path = dir.path().join("a.jsonl");
        std::fs::write(&first.source_path, "a").unwrap();
        let mut second = make_session("sess-b", "beta");
        second.source_path = dir.path().join("b.jsonl");
        std::fs::write(&second.source_path, "b").unwrap();

        stub.add(first.clone(), vec![make_message("a", "alpha survives")]);
        stub.add(second.clone(), vec![make_message("b", "beta removed")]);

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index
            .build_index(&[first.clone(), second], &providers, &tx)
            .unwrap();

        let stats = index.build_index(&[first], &providers, &tx).unwrap();
        assert_eq!(stats.removed, 1);
        assert_eq!(index.search("alpha", 10).unwrap().len(), 1);
        assert!(index.search("beta", 10).unwrap().is_empty());
    }

    #[test]
    fn file_fingerprint_includes_content_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, "first").unwrap();
        let first = file_fingerprint(&path);
        std::fs::write(&path, "second").unwrap();
        let second = file_fingerprint(&path);

        assert_ne!(first.sha256, second.sha256);
    }

    #[test]
    fn session_ids_with_messages_filters_by_role() {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();

        let stub = StubProvider::new(Provider::ClaudeCode);
        let s_user = make_session("sess-user-only", "alpha");
        let s_mixed = make_session("sess-mixed", "beta");
        stub.add(s_user.clone(), vec![make_message("u-1", "user only msg")]);
        let mut asst = make_message("a-1", "assistant reply");
        asst.role = Role::Assistant;
        stub.add(s_mixed.clone(), vec![make_message("u-2", "user msg"), asst]);

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let s_mixed_key = s_mixed.identity_key();
        let sessions = vec![s_user, s_mixed];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index.build_index(&sessions, &providers, &tx).unwrap();

        let only_assistant = index
            .session_ids_with_messages(Some(Role::Assistant), false)
            .unwrap();
        assert_eq!(only_assistant.len(), 1);
        assert!(only_assistant.contains(&s_mixed_key));

        let any_user = index
            .session_ids_with_messages(Some(Role::User), false)
            .unwrap();
        assert_eq!(any_user.len(), 2);

        // No filter → empty set (caller treats as "no constraint").
        let none = index.session_ids_with_messages(None, false).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn session_ids_with_messages_filters_by_has_tool_call() {
        let dir = tempdir().unwrap();
        let index = SearchIndex::open_or_create(dir.path()).unwrap();

        let stub = StubProvider::new(Provider::ClaudeCode);
        let s_plain = make_session("sess-plain", "alpha");
        let s_with_tool = make_session("sess-with-tool", "beta");
        stub.add(s_plain.clone(), vec![make_message("p-1", "no tool here")]);
        let mut tool_msg = make_message("t-1", "calling tool");
        tool_msg.role = Role::Assistant;
        tool_msg
            .content
            .push(ContentBlock::ToolUse(crate::model::ToolCall {
                id: "tc-1".to_string(),
                name: "fs.read".to_string(),
                arguments: "{\"path\":\"/x\"}".to_string(),
            }));
        stub.add(s_with_tool.clone(), vec![tool_msg]);

        let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
        let s_with_tool_key = s_with_tool.identity_key();
        let sessions = vec![s_plain, s_with_tool];
        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index.build_index(&sessions, &providers, &tx).unwrap();

        let with_tools = index.session_ids_with_messages(None, true).unwrap();
        assert_eq!(with_tools.len(), 1);
        assert!(with_tools.contains(&s_with_tool_key));

        // Combined: assistant role AND has-tool-call → still just the tool session.
        let combined = index
            .session_ids_with_messages(Some(Role::Assistant), true)
            .unwrap();
        assert_eq!(combined.len(), 1);
        assert!(combined.contains(&s_with_tool_key));
    }

    #[test]
    fn cosine_similarity_handles_identical_orthogonal_and_zero_vectors() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [1.0_f32, 0.0, 0.0];
        let c = [0.0_f32, 1.0, 0.0];
        let z = [0.0_f32, 0.0, 0.0];

        // Identical → 1.0 (within float tolerance).
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
        // Orthogonal → 0.0.
        assert!(cosine_similarity(&a, &c).abs() < 1e-6);
        // Zero norm on either side → exactly 0.0 (the function returns 0.0
        // literally, not via float arithmetic, so equality is safe).
        assert!(cosine_similarity(&a, &z).abs() < 1e-6);
        assert!(cosine_similarity(&z, &z).abs() < 1e-6);
        // Mismatched lengths or empty inputs → exactly 0.0 (early return).
        assert!(cosine_similarity(&a, &[1.0_f32, 0.0]).abs() < 1e-6);
        assert!(cosine_similarity(&[][..], &[][..]).abs() < 1e-6);
    }

    #[test]
    fn search_hybrid_falls_back_to_lexical_when_weight_is_zero() {
        let (_dir, index) = build_tiny_index();
        let lex = index
            .search_with_filters("tantivy", 10, &SearchFilters::default())
            .unwrap();
        let hybrid = index
            .search_hybrid(
                "tantivy",
                &[SemanticCandidate {
                    message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
                    message_id: "m-4".to_string(),
                    similarity: 0.9,
                }],
                10,
                &SearchFilters::default(),
                0.0,
                50,
            )
            .unwrap();
        // weight=0 must yield exactly the lexical result, including identical
        // BM25 scores — not RRF scores.
        assert_eq!(hybrid.len(), lex.len());
        for (a, b) in hybrid.iter().zip(lex.iter()) {
            assert_eq!(a.message_id, b.message_id);
            assert!((a.score - b.score).abs() < 1e-6);
        }
    }

    #[test]
    fn search_hybrid_falls_back_to_lexical_when_semantic_pool_is_empty() {
        let (_dir, index) = build_tiny_index();
        let lex = index
            .search_with_filters("tantivy", 10, &SearchFilters::default())
            .unwrap();
        let hybrid = index
            .search_hybrid("tantivy", &[], 10, &SearchFilters::default(), 0.5, 50)
            .unwrap();
        assert_eq!(hybrid.len(), lex.len());
        for (a, b) in hybrid.iter().zip(lex.iter()) {
            assert_eq!(a.message_id, b.message_id);
        }
    }

    #[test]
    fn search_hybrid_promotes_semantic_only_hits() {
        // "tantivy" only matches m-3 lexically. If we pretend the embedding
        // model thinks m-4 ("fastembed semantic vectors") is the top semantic
        // match, RRF should return BOTH — proving hybrid surfaces hits the
        // lexical pass alone wouldn't.
        let (_dir, index) = build_tiny_index();
        let semantic = vec![
            SemanticCandidate {
                message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
                message_id: "m-4".to_string(),
                similarity: 0.95,
            },
            SemanticCandidate {
                message_key: make_session("sess-2", "beta").message_key(0, "m-3"),
                message_id: "m-3".to_string(),
                similarity: 0.80,
            },
        ];
        let hybrid = index
            .search_hybrid("tantivy", &semantic, 10, &SearchFilters::default(), 0.5, 50)
            .unwrap();
        let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
        assert!(ids.contains(&"m-3"), "lexical hit must survive: {ids:?}");
        assert!(
            ids.contains(&"m-4"),
            "semantic-only hit must surface: {ids:?}"
        );
        // m-3 appears in both pools → its fused score should beat m-4 (which
        // is semantic-only) when weight=0.5.
        let m3_score = hybrid.iter().find(|h| h.message_id == "m-3").unwrap().score;
        let m4_score = hybrid.iter().find(|h| h.message_id == "m-4").unwrap().score;
        assert!(
            m3_score > m4_score,
            "lexical+semantic hit (m-3, score {m3_score}) should outrank semantic-only (m-4, score {m4_score})"
        );
    }

    fn make_note(id: i64, session_ref: &str, body: &str, updated_at: &str) -> Note {
        Note {
            id,
            session_ref: session_ref.to_string(),
            body: body.to_string(),
            created_at: updated_at.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn index_notes_makes_bodies_searchable_with_kind_note() {
        let (_dir, index) = build_tiny_index();
        let notes = vec![make_note(
            1,
            "claude-code/sess-1#3",
            "investigate xylophone bug",
            "2026-01-01T00:00:00Z",
        )];
        let stats = index.index_notes(&notes).unwrap();
        assert_eq!(stats.added, 1);
        assert_eq!(stats.unchanged, 0);

        let hits = index
            .search_with_filters("xylophone", 10, &SearchFilters::default())
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, HitKind::Note);
        assert_eq!(hits[0].note_id, Some(1));
        assert_eq!(
            hits[0].note_session_ref.as_deref(),
            Some("claude-code/sess-1#3")
        );
    }

    #[test]
    fn index_notes_is_incremental_on_unchanged_updated_at() {
        let (_dir, index) = build_tiny_index();
        let notes = vec![make_note(
            42,
            "claude-code/sess-1",
            "first version",
            "2026-01-01T00:00:00Z",
        )];
        let s1 = index.index_notes(&notes).unwrap();
        assert_eq!(s1.added, 1);
        let s2 = index.index_notes(&notes).unwrap();
        // Re-indexing with the same updated_at must skip everything.
        assert_eq!(s2.added, 0);
        assert_eq!(s2.updated, 0);
        assert_eq!(s2.unchanged, 1);
    }

    #[test]
    fn index_notes_replaces_doc_when_updated_at_advances() {
        let (_dir, index) = build_tiny_index();
        let v1 = vec![make_note(
            7,
            "claude-code/sess-1",
            "old text marker7",
            "2026-01-01T00:00:00Z",
        )];
        index.index_notes(&v1).unwrap();
        let v2 = vec![make_note(
            7,
            "claude-code/sess-1",
            "new text marker7",
            "2026-02-01T00:00:00Z",
        )];
        let stats = index.index_notes(&v2).unwrap();
        assert_eq!(stats.updated, 1);

        // Old body must no longer match.
        let old_hits = index
            .search_with_filters("old", 10, &SearchFilters::default())
            .unwrap();
        assert!(
            old_hits.iter().all(|h| h.kind != HitKind::Note),
            "old note body should have been replaced: {old_hits:?}"
        );
        // New body must match.
        let new_hits = index
            .search_with_filters("new", 10, &SearchFilters::default())
            .unwrap();
        assert!(new_hits.iter().any(|h| h.kind == HitKind::Note));
    }

    #[test]
    fn index_notes_prunes_removed_rows() {
        let (_dir, index) = build_tiny_index();
        let v1 = vec![make_note(
            9,
            "claude-code/sess-1",
            "soon-to-vanish marker9",
            "2026-01-01T00:00:00Z",
        )];
        index.index_notes(&v1).unwrap();
        let stats = index.index_notes(&[]).unwrap();
        assert_eq!(stats.removed, 1);
        let hits = index
            .search_with_filters("soon-to-vanish", 10, &SearchFilters::default())
            .unwrap();
        assert!(
            hits.iter().all(|h| h.kind != HitKind::Note),
            "pruned note must not match: {hits:?}"
        );
    }

    #[test]
    fn search_hybrid_applies_filters_to_semantic_candidates() {
        // Filter to a project that only contains sess-1, but feed in semantic
        // candidates that include m-4 (in sess-2). Hybrid must drop m-4 — the
        // filter applies symmetrically to both ranking sources.
        let (_dir, index) = build_tiny_index();
        let filters = SearchFilters {
            project: Some("alpha".to_string()),
            ..SearchFilters::default()
        };
        let semantic = vec![
            SemanticCandidate {
                message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
                message_id: "m-4".to_string(),
                similarity: 0.95,
            },
            SemanticCandidate {
                message_key: make_session("sess-1", "alpha").message_key(0, "m-1"),
                message_id: "m-1".to_string(),
                similarity: 0.70,
            },
        ];
        let hybrid = index
            .search_hybrid("rust", &semantic, 10, &filters, 0.5, 50)
            .unwrap();
        let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
        assert!(
            !ids.contains(&"m-4"),
            "project filter must drop m-4 from semantic pool: {ids:?}"
        );
        assert!(
            ids.contains(&"m-1"),
            "filter-passing hit must remain: {ids:?}"
        );
    }
}
