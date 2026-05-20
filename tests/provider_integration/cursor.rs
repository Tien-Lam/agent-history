use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::cursor::CursorProvider;
use aghist::provider::HistoryProvider;

use super::common;

#[test]
fn cursor_discover_and_load_from_generated_fixture() {
    let fixture = common::fixtures::cursor::cursor_single_session(4);
    let provider = CursorProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "comp-gen-001");
    assert_eq!(s.provider, Provider::Cursor);
    assert_eq!(s.project_name.as_deref(), Some("myapp"));
    assert_eq!(s.message_count, 4);

    let messages = provider.load_messages(s).unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
    assert!(matches!(
        &messages[0].content[0],
        ContentBlock::Text(t) if t.contains("User message 0")
    ));
}
