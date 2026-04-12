use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, INDEXED, STORED, STRING, TEXT};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::action::Action;
use crate::model::{ContentBlock, Message, Session};
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

pub struct IndexStats {
    pub sessions_indexed: usize,
    pub messages_indexed: usize,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    f_session_id: Field,
    f_message_id: Field,
    f_provider: Field,
    f_project: Field,
    f_role: Field,
    f_content: Field,
    f_timestamp: Field,
    index_dir: PathBuf,
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
        let f_role = builder.add_text_field("role", STRING | STORED);
        let f_content = builder.add_text_field("content", TEXT | STORED);
        let f_timestamp = builder.add_i64_field("timestamp", INDEXED | STORED);
        let schema = builder.build();

        let index = if index_dir.join("meta.json").exists() {
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
            f_role,
            f_content,
            f_timestamp,
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
        let mut sessions_indexed = 0;
        let mut messages_indexed = 0;

        for (i, session) in sessions.iter().enumerate() {
            let path_key = session.source_path.to_string_lossy().into_owned();
            let current_mtime = file_mtime(&session.source_path);

            if let Some(&cached_mtime) = manifest.sessions.get(&path_key) {
                if cached_mtime == current_mtime {
                    let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
                    continue;
                }
            }

            writer.delete_term(tantivy::Term::from_field_text(
                self.f_session_id,
                &session.id.0,
            ));

            if let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) {
                if let Ok(messages) = provider.load_messages(session) {
                    for msg in &messages {
                        let text = extract_text(msg);
                        if text.is_empty() {
                            continue;
                        }
                        let mut doc = TantivyDocument::default();
                        doc.add_text(self.f_session_id, &session.id.0);
                        doc.add_text(self.f_message_id, &msg.id.0);
                        doc.add_text(self.f_provider, session.provider.as_str());
                        doc.add_text(
                            self.f_project,
                            session.project_name.as_deref().unwrap_or(""),
                        );
                        doc.add_text(self.f_role, msg.role.as_str());
                        doc.add_text(self.f_content, &text);
                        doc.add_i64(self.f_timestamp, msg.timestamp.timestamp());
                        writer.add_document(doc)?;
                        messages_indexed += 1;
                    }
                }
            }

            manifest.sessions.insert(path_key, current_mtime);
            sessions_indexed += 1;
            let _ = progress_tx.send(Action::IndexProgress(i + 1, total));
        }

        writer.commit()?;
        self.save_manifest(&manifest)?;

        Ok(IndexStats {
            sessions_indexed,
            messages_indexed,
        })
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>, SearchError> {
        if query_str.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        let parser = QueryParser::for_index(&self.index, vec![self.f_content, self.f_project]);
        let query = parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).order_by_score())?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let session_id = field_text(&doc, self.f_session_id);
            let message_id = field_text(&doc, self.f_message_id);
            let content = field_text(&doc, self.f_content);
            let snippet = make_snippet(&content, query_str, 120);

            hits.push(SearchHit {
                session_id,
                message_id,
                snippet,
                score,
            });
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

fn extract_text(message: &Message) -> String {
    message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect::<Vec<_>>()
        .join("\n")
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
