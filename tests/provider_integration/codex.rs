use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::HistoryProvider;

use super::common::helpers::fixtures_dir;

#[test]
fn codex_discover_sessions() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "rollout-test123");
    assert_eq!(s.provider, Provider::CodexCli);
    assert_eq!(s.message_count, 3);
    assert!(s.summary.as_ref().unwrap().contains("error handling"));
}

#[test]
fn codex_load_messages() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0].id.0, "codex-turn-1");
    assert!(messages.iter().all(|message| !message.id.0.is_empty()));

    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[2].role, Role::Tool);
    assert!(matches!(&messages[2].content[0], ContentBlock::ToolUse(tc) if tc.name == "EditFile"));

    assert_eq!(messages[3].role, Role::System);
    assert!(
        matches!(&messages[3].content[0], ContentBlock::Error(e) if e.contains("File not found"))
    );

    assert_eq!(messages[4].role, Role::Assistant);
}
