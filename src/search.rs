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

fn message_has_tool_call(message: &Message) -> bool {
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
