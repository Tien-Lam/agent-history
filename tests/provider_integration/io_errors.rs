use std::path::PathBuf;

use aghist::model::{Provider, Session, SessionId};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use super::common;

#[test]
fn load_messages_from_deleted_file_returns_error() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let provider = ClaudeCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider
        .discover_sessions()
        .expect("should discover sessions");
    assert!(!sessions.is_empty());

    std::fs::remove_file(&sessions[0].source_path).expect("should delete fixture file");

    let result = provider.load_messages(&sessions[0]);
    assert!(
        result.is_err(),
        "should return error when session file is deleted"
    );
}

#[test]
fn load_messages_from_empty_file_returns_empty() {
    let dir = tempfile::TempDir::new().expect("should create temp dir");
    let base = dir.path().join(".claude");
    let project_dir = base.join("projects").join("test-project");
    std::fs::create_dir_all(&project_dir).expect("should create project dir");

    std::fs::write(
        base.join("history.jsonl"),
        r#"{"display":"test","timestamp":1700000000000,"project":"test-project","sessionId":"empty-session"}"#,
    )
    .expect("should write history");

    std::fs::write(project_dir.join("empty-session.jsonl"), "").expect("should write empty file");

    let provider = ClaudeCodeProvider::new(vec![base]);
    let sessions = provider.discover_sessions().expect("should discover");
    assert!(
        sessions.is_empty(),
        "empty session file should be filtered out"
    );
}

#[test]
fn discover_survives_unreadable_session_file() {
    let fixture = common::fixtures::claude::claude_multi_session(3, 2);
    let provider = ClaudeCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider
        .discover_sessions()
        .expect("should discover sessions");
    let original_count = sessions.len();
    assert!(
        original_count >= 2,
        "need at least 2 sessions for this test"
    );

    std::fs::write(&sessions[0].source_path, "not valid json at all\n").expect("should truncate");

    let sessions2 = provider.discover_sessions().expect("should still discover");
    assert!(
        sessions2.len() >= original_count - 1,
        "should discover at least {} sessions after corrupting one, got {}",
        original_count - 1,
        sessions2.len()
    );
}

#[test]
fn load_messages_nonexistent_source_path() {
    let fake_session = Session {
        id: SessionId("fake-session".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("fake".into()),
        git_branch: None,
        started_at: chrono::Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 0,
        source_path: PathBuf::from("/nonexistent/session.jsonl"),
    };

    let provider = ClaudeCodeProvider::new(vec![PathBuf::from("/nonexistent")]);
    let result = provider.load_messages(&fake_session);
    assert!(result.is_err(), "should error on nonexistent source_path");
}
