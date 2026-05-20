use aghist::export;

use super::load_fixture_session;

#[test]
fn json_is_valid_and_has_structure() {
    let (session, messages) = load_fixture_session();
    let json_str = export::to_json(&session, &messages);

    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("should be valid JSON");

    assert!(parsed.get("session").is_some(), "should have session key");
    assert!(parsed.get("messages").is_some(), "should have messages key");
}

#[test]
fn json_session_has_required_fields() {
    let (session, messages) = load_fixture_session();
    let json_str = export::to_json(&session, &messages);

    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let sess = parsed.get("session").unwrap();

    assert!(sess.get("id").is_some(), "session should have id");
    assert!(
        sess.get("provider").is_some(),
        "session should have provider"
    );
    assert!(
        sess.get("started_at").is_some(),
        "session should have started_at"
    );
}

#[test]
fn json_messages_preserve_content() {
    let (session, messages) = load_fixture_session();
    let json_str = export::to_json(&session, &messages);

    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let msgs = parsed.get("messages").unwrap().as_array().unwrap();

    assert_eq!(
        msgs.len(),
        messages.len(),
        "JSON should have same number of messages"
    );

    for msg in msgs {
        assert!(msg.get("role").is_some(), "each message should have role");
        assert!(
            msg.get("content").is_some(),
            "each message should have content"
        );
        assert!(
            msg.get("timestamp").is_some(),
            "each message should have timestamp"
        );
    }
}

#[test]
fn json_content_blocks_are_tagged() {
    let (session, messages) = load_fixture_session();
    let json_str = export::to_json(&session, &messages);

    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let msgs = parsed.get("messages").unwrap().as_array().unwrap();

    for msg in msgs {
        let content = msg.get("content").unwrap().as_array().unwrap();
        for block in content {
            assert!(
                block.get("type").is_some(),
                "each content block should have a type tag"
            );
        }
    }
}
