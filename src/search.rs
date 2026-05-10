use std::collections::HashMap;
use std::fs;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, Value, INDEXED, STORED, STRING, TEXT};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

pub use tantivy::query::Explanation;

use crate::action::Action;
use crate::model::{ContentBlock, Message, Provider, Role, Session};
use crate::provider::HistoryProvider;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub session_id: String,
    pub message_id: String,
    pub snippet: String,
    pub score: f32,
}

/// One semantic candidate sourced from cosine similarity over the embedding
/// store. Callers rank the full store and pass the top-N here; `search.rs`
/// stays ignorant of the embedding pipeline.
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub message_id: String,
    pub similarity: f32,
}

/// Reciprocal Rank Fusion smoothing constant. 60 is the value from Cormack
/// et al.'s original paper and the one most production hybrid-search systems
/// use; large enough to dampen the penalty for rank-1 vs rank-2 differences,
/// small enough that rank still matters.
pub const RRF_K: f32 = 60.0;

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

#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    /// Sessions written this pass (added + updated).
    pub sessions_indexed: usize,
    /// Messages written this pass.
    pub messages_indexed: usize,
    /// Sessions never seen by the manifest before.
    pub added: usize,
    /// Sessions that existed in the manifest but had a newer source mtime.
    pub updated: usize,
    /// Sessions that the manifest already had at the current mtime — skipped.
    pub unchanged: usize,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    f_session_id: Field,
    f_message_id: Field,
    f_provider: Field,
    f_project: Field,
    f_project_raw: Field,
    f_role: Field,
    f_content: Field,
    f_tool_output: Field,
    f_timestamp: Field,
    f_has_tool_call: Field,
    index_dir: PathBuf,
}

/// Server-side filters applied alongside a `search` query. Empty fields mean
/// "do not filter on this dimension". `since`/`until` are inclusive bounds on
/// the message timestamp; `project` is a case-insensitive substring match.
#[derive(Debug, Default, Clone)]
pub struct SearchFilters {
    pub provider: Option<Provider>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub project: Option<String>,
    pub role: Option<Role>,
    pub has_tool_call: bool,
}

impl SearchFilters {
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.since.is_none()
            && self.until.is_none()
            && self.project.is_none()
            && self.role.is_none()
            && !self.has_tool_call
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    sessions: HashMap<String, u64>,
}

impl SearchIndex {
    pub fn open_or_create(index_dir: &Path) -> Result<Self, SearchError> {
        fs::create_dir_all(index_dir)?;

        let mut builder = Schema::builder();
        let f_session_id = builder.add_text_field("session_id", STRING | STORED);
        let f_message_id = builder.add_text_field("message_id", STRING | STORED);
        let f_provider = builder.add_text_field("provider", STRING | STORED);
        let f_project = builder.add_text_field("project", TEXT | STORED);
        let f_project_raw = builder.add_text_field("project_raw", STRING | STORED);
        let f_role = builder.add_text_field("role", STRING | STORED);
        let f_content = builder.add_text_field("content", TEXT | STORED);
        let f_tool_output = builder.add_text_field("tool_output", TEXT | STORED);
        let f_timestamp = builder.add_i64_field("timestamp", INDEXED | STORED);
        let f_has_tool_call = builder.add_i64_field("has_tool_call", INDEXED | STORED);
        let schema = builder.build();

        let meta_path = index_dir.join("meta.json");
        // The on-disk index is a cache; if its schema predates a field we now
        // need (e.g. tool_output was added), rebuild from scratch instead of
        // failing to open.
        if meta_path.exists() && !schema_matches(index_dir, &schema) {
            wipe_index_dir(index_dir)?;
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

        Ok(Self {
            index,
            reader,
            f_session_id,
            f_message_id,
            f_provider,
            f_project,
            f_project_raw,
            f_role,
            f_content,
            f_tool_output,
            f_timestamp,
            f_has_tool_call,
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

        let total = sessions.len();
        let mut stats = IndexStats::default();

        for (i, session) in sessions.iter().enumerate() {
            let path_key = session.source_path.to_string_lossy().into_owned();
            let current_mtime = file_mtime(&session.source_path);

            let existing_mtime = manifest.sessions.get(&path_key).copied();
            match existing_mtime {
                Some(cached) if cached == current_mtime => {
                    stats.unchanged += 1;
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
                Some(_) => stats.updated += 1,
                None => stats.added += 1,
            }

            writer.delete_term(tantivy::Term::from_field_text(
                self.f_session_id,
                &session.id.0,
            ));

            if let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) {
                if let Ok(messages) = provider.load_messages(session) {
                    for msg in &messages {
                        let content = extract_content(msg);
                        let tool_output = extract_tool_output(msg);
                        if content.is_empty() && tool_output.is_empty() {
                            continue;
                        }
                        let project = session.project_name.as_deref().unwrap_or("");
                        let has_tool_call = i64::from(message_has_tool_call(msg));
                        let mut doc = TantivyDocument::default();
                        doc.add_text(self.f_session_id, &session.id.0);
                        doc.add_text(self.f_message_id, &msg.id.0);
                        doc.add_text(self.f_provider, session.provider.slug());
                        doc.add_text(self.f_project, project);
                        doc.add_text(self.f_project_raw, project);
                        doc.add_text(self.f_role, msg.role.slug());
                        doc.add_text(self.f_content, &content);
                        doc.add_text(self.f_tool_output, &tool_output);
                        doc.add_i64(self.f_timestamp, msg.timestamp.timestamp());
                        doc.add_i64(self.f_has_tool_call, has_tool_call);
                        writer.add_document(doc)?;
                        stats.messages_indexed += 1;
                    }
                }
            }

            manifest.sessions.insert(path_key, current_mtime);
            stats.sessions_indexed += 1;
            let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
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

        let sem_ids: Vec<&str> = semantic_ranked
            .iter()
            .take(pool)
            .map(|c| c.message_id.as_str())
            .collect();
        let semantic: Vec<SearchHit> =
            self.fetch_filtered_by_message_ids(query_str, &sem_ids, filters)?;

        // Tuple is (lexical rank 1-based, semantic rank 1-based, hit). A None
        // rank means "absent from that pool" → that side contributes 0 to RRF.
        let mut by_id: HashMap<String, (Option<usize>, Option<usize>, SearchHit)> =
            HashMap::new();
        for (i, hit) in lexical.into_iter().enumerate() {
            by_id.insert(hit.message_id.clone(), (Some(i + 1), None, hit));
        }
        for (i, hit) in semantic.into_iter().enumerate() {
            by_id
                .entry(hit.message_id.clone())
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
                .then_with(|| a.session_id.cmp(&b.session_id))
                .then_with(|| a.message_id.cmp(&b.message_id))
        });
        fused.truncate(limit);
        Ok(fused)
    }

    /// Look up indexed messages by `message_id`, applying the same filter
    /// clauses as [`Self::search_inner`]. Used by hybrid search to validate
    /// semantic candidates against server-side filters before fusing.
    ///
    /// The returned vector preserves the input order (typically similarity
    /// DESC), with non-matching ids dropped — so callers can use position as
    /// the semantic rank.
    fn fetch_filtered_by_message_ids(
        &self,
        query_str: &str,
        ids: &[&str],
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHit>, SearchError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        // OR of message_id terms — Tantivy doesn't have an IN query, so we
        // build a Should-clause boolean.
        let id_clauses: Vec<(Occur, Box<dyn Query>)> = ids
            .iter()
            .map(|id| {
                let term = Term::from_field_text(self.f_message_id, id);
                (
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as Box<dyn Query>,
                )
            })
            .collect();
        let id_query: Box<dyn Query> = Box::new(BooleanQuery::new(id_clauses));

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(5);
        clauses.push((Occur::Must, id_query));

        if let Some(provider) = filters.provider {
            let term = Term::from_field_text(self.f_provider, provider.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if let Some(role) = filters.role {
            let term = Term::from_field_text(self.f_role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.has_tool_call {
            let term = Term::from_field_i64(self.f_has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.since.is_some() || filters.until.is_some() {
            let lower = filters.since.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.f_timestamp, t.timestamp()))
            });
            let upper = filters.until.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.f_timestamp, t.timestamp()))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }

        let combined: Box<dyn Query> = Box::new(BooleanQuery::new(clauses));

        // Cap at the input length — we have at most one hit per requested id.
        let top_docs = searcher.search(
            &combined,
            &TopDocs::with_limit(ids.len()).order_by_score(),
        )?;

        let project_needle = filters
            .project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty());

        let mut by_msg_id: HashMap<String, SearchHit> = HashMap::new();
        for (_score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(needle) = &project_needle {
                let project = field_text(&doc, self.f_project_raw);
                if !project.to_lowercase().contains(needle) {
                    continue;
                }
            }
            let session_id = field_text(&doc, self.f_session_id);
            let message_id = field_text(&doc, self.f_message_id);
            let content = field_text(&doc, self.f_content);
            let tool_output = field_text(&doc, self.f_tool_output);
            let snippet = best_snippet(&content, &tool_output, query_str, 120);
            by_msg_id.insert(
                message_id.clone(),
                SearchHit {
                    session_id,
                    message_id,
                    snippet,
                    score: 0.0,
                },
            );
        }

        // Drain in input order. Matches positionally to similarity rank.
        Ok(ids
            .iter()
            .filter_map(|id| by_msg_id.remove(*id))
            .collect())
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
            vec![self.f_content, self.f_project, self.f_tool_output],
        );
        let user_query = parser.parse_query(query_str)?;

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(6);
        clauses.push((Occur::Must, user_query));

        if let Some(provider) = filters.provider {
            let term = Term::from_field_text(self.f_provider, provider.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if let Some(role) = filters.role {
            let term = Term::from_field_text(self.f_role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.has_tool_call {
            let term = Term::from_field_i64(self.f_has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.since.is_some() || filters.until.is_some() {
            let lower = filters.since.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.f_timestamp, t.timestamp()))
            });
            let upper = filters.until.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.f_timestamp, t.timestamp()))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }

        let combined: Box<dyn Query> = if clauses.len() == 1 {
            clauses.into_iter().next().expect("one clause").1
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        // Project is post-filtered; over-fetch to keep results stable when a
        // restrictive project filter would otherwise prune the limit-N window.
        let project_needle = filters
            .project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty());
        let fetch_limit = if project_needle.is_some() {
            limit.saturating_mul(8).max(limit)
        } else {
            limit
        };

        let top_docs =
            searcher.search(&combined, &TopDocs::with_limit(fetch_limit).order_by_score())?;

        let mut hits = Vec::with_capacity(top_docs.len().min(limit));
        for (score, addr) in top_docs {
            if hits.len() >= limit {
                break;
            }
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(needle) = &project_needle {
                let project = field_text(&doc, self.f_project_raw);
                if !project.to_lowercase().contains(needle) {
                    continue;
                }
            }
            let session_id = field_text(&doc, self.f_session_id);
            let message_id = field_text(&doc, self.f_message_id);
            let content = field_text(&doc, self.f_content);
            let tool_output = field_text(&doc, self.f_tool_output);
            let snippet = best_snippet(&content, &tool_output, query_str, 120);

            let explanation = if explain {
                Some(combined.explain(&searcher, addr)?)
            } else {
                None
            };

            hits.push((
                SearchHit {
                    session_id,
                    message_id,
                    snippet,
                    score,
                },
                explanation,
            ));
        }

        Ok(hits)
    }

    pub fn clear(&self) -> Result<(), SearchError> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        writer.commit()?;
        let _ = fs::remove_file(self.index_dir.join("manifest.json"));
        Ok(())
    }

    pub fn default_index_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("AGHIST_INDEX_DIR") {
            return PathBuf::from(dir);
        }
        directories::ProjectDirs::from("", "", "aghist")
            .map_or_else(|| PathBuf::from(".aghist-index"), |d| d.cache_dir().join("search-index"))
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

fn extract_content(message: &Message) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                parts.push(t.as_str());
            }
            ContentBlock::CodeBlock { code, .. } => parts.push(code.as_str()),
            ContentBlock::ToolUse(tc) => parts.push(tc.arguments.as_str()),
            ContentBlock::ToolResult(_) => {}
        }
    }
    parts.join("\n")
}

pub fn message_has_tool_call(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse(_)))
}

fn extract_tool_output(message: &Message) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for block in &message.content {
        if let ContentBlock::ToolResult(tr) = block {
            parts.push(tr.output.as_str());
        }
    }
    parts.join("\n")
}

fn schema_matches(index_dir: &Path, expected: &Schema) -> bool {
    Index::open_in_dir(index_dir).is_ok_and(|idx| &idx.schema() == expected)
}

fn wipe_index_dir(index_dir: &Path) -> Result<(), SearchError> {
    for entry in fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn best_snippet(content: &str, tool_output: &str, query: &str, max_len: usize) -> String {
    // Prefer whichever stored field actually contains the query, so a hit on
    // tool_output doesn't return an empty/unrelated snippet from content.
    let q_lower = query.to_lowercase();
    if tool_output.to_lowercase().contains(&q_lower) {
        make_snippet(tool_output, query, max_len)
    } else if !content.is_empty() {
        make_snippet(content, query, max_len)
    } else {
        make_snippet(tool_output, query, max_len)
    }
}

fn field_text(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn file_mtime(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn make_snippet(content: &str, query: &str, max_len: usize) -> String {
    let lower = content.to_lowercase();
    let q = query.to_lowercase();

    let pos = lower.find(&q).unwrap_or(0);
    let mut start = pos.saturating_sub(max_len / 2);
    let mut end = (start + max_len).min(content.len());

    while start > 0 && !content.is_char_boundary(start) {
        start -= 1;
    }
    while end < content.len() && !content.is_char_boundary(end) {
        end += 1;
    }

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(&content[start..end]);
    if end < content.len() {
        snippet.push_str("...");
    }
    snippet.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, MessageId, Provider, Role, SessionId};
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
                .insert(session.id.0.clone(), messages);
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
                .get(&session.id.0)
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
        let lex = index.search_with_filters("tantivy", 10, &SearchFilters::default()).unwrap();
        let hybrid = index
            .search_hybrid(
                "tantivy",
                &[SemanticCandidate {
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
        let lex = index.search_with_filters("tantivy", 10, &SearchFilters::default()).unwrap();
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
            SemanticCandidate { message_id: "m-4".to_string(), similarity: 0.95 },
            SemanticCandidate { message_id: "m-3".to_string(), similarity: 0.80 },
        ];
        let hybrid = index
            .search_hybrid("tantivy", &semantic, 10, &SearchFilters::default(), 0.5, 50)
            .unwrap();
        let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
        assert!(ids.contains(&"m-3"), "lexical hit must survive: {ids:?}");
        assert!(ids.contains(&"m-4"), "semantic-only hit must surface: {ids:?}");
        // m-3 appears in both pools → its fused score should beat m-4 (which
        // is semantic-only) when weight=0.5.
        let m3_score = hybrid.iter().find(|h| h.message_id == "m-3").unwrap().score;
        let m4_score = hybrid.iter().find(|h| h.message_id == "m-4").unwrap().score;
        assert!(
            m3_score > m4_score,
            "lexical+semantic hit (m-3, score {m3_score}) should outrank semantic-only (m-4, score {m4_score})"
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
            SemanticCandidate { message_id: "m-4".to_string(), similarity: 0.95 },
            SemanticCandidate { message_id: "m-1".to_string(), similarity: 0.70 },
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
