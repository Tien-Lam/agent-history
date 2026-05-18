use crate::action::Action;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};

use super::fingerprint::file_fingerprint;
use super::*;

use chrono::TimeZone;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::tempdir;

mod filters;
mod hybrid;
mod indexing;
mod notes;

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
