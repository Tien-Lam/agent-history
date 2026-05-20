mod common;

use std::path::PathBuf;

use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use common::helpers::fixtures_dir;

#[path = "export/dispatch.rs"]
mod dispatch;
#[path = "export/format.rs"]
mod format;
#[path = "export/html.rs"]
mod html;
#[path = "export/json.rs"]
mod json;
#[path = "export/markdown.rs"]
mod markdown;
#[path = "export/notes.rs"]
mod notes;
#[path = "export/unicode.rs"]
mod unicode;

fn load_fixture_session() -> (aghist::model::Session, Vec<aghist::model::Message>) {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let session = sessions
        .into_iter()
        .next()
        .expect("fixture has at least one session");
    let messages = provider.load_messages(&session).unwrap();
    (session, messages)
}

fn make_note(id: i64, session_ref: &str, body: &str) -> aghist::metadata::Note {
    aghist::metadata::Note {
        id,
        session_ref: session_ref.to_string(),
        body: body.to_string(),
        created_at: "2026-05-13T12:00:00Z".to_string(),
        updated_at: "2026-05-13T12:00:00Z".to_string(),
    }
}

fn sample_session() -> (aghist::model::Session, Vec<aghist::model::Message>) {
    use aghist::model::*;
    use chrono::Utc;

    let session = Session {
        id: SessionId("abc-123".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("Demo".into()),
        git_branch: None,
        started_at: Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 3,
        source_path: PathBuf::from("/tmp/test"),
    };
    let messages = (1..=3)
        .map(|i| Message {
            id: MessageId(format!("m{i}")),
            role: if i % 2 == 1 {
                Role::User
            } else {
                Role::Assistant
            },
            timestamp: Utc::now(),
            content: vec![ContentBlock::Text(format!("turn-{i} body"))],
            model: None,
            token_usage: None,
        })
        .collect();
    (session, messages)
}
