use std::path::PathBuf;

use aghist::export;

use super::load_fixture_session;

#[test]
fn html_is_self_contained() {
    let (session, messages) = load_fixture_session();
    let html = export::to_html(&session, &messages);

    assert!(html.contains("<!DOCTYPE html>"), "should have doctype");
    assert!(html.contains("<style>"), "should have inline CSS");
    assert!(html.contains("</html>"), "should be closed HTML");
    assert!(
        !html.contains("<link rel=\"stylesheet\""),
        "should not link external CSS"
    );
    assert!(
        !html.contains("<script src="),
        "should not link external scripts"
    );
}

#[test]
fn html_contains_message_content() {
    let (session, messages) = load_fixture_session();
    let html = export::to_html(&session, &messages);

    assert!(
        html.contains("class=\"message user\""),
        "should have user messages"
    );
    assert!(
        html.contains("class=\"message assistant\""),
        "should have assistant messages"
    );
    assert!(html.contains("class=\"role\""), "should have role labels");
}

#[test]
fn html_uses_details_for_tool_calls() {
    let (session, messages) = load_fixture_session();
    let html = export::to_html(&session, &messages);

    let has_tool_use = messages
        .iter()
        .flat_map(|m| &m.content)
        .any(|c| matches!(c, aghist::model::ContentBlock::ToolUse(_)));

    if has_tool_use {
        assert!(
            html.contains("<details>"),
            "tool calls should use <details> tags"
        );
        assert!(
            html.contains("<summary>Tool:"),
            "tool calls should have summary with tool name"
        );
    }
}

#[test]
fn html_escapes_special_characters() {
    use aghist::model::*;
    use chrono::Utc;

    let session = Session {
        id: SessionId("test".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("<script>alert('xss')</script>".into()),
        git_branch: None,
        started_at: Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from("/tmp/test"),
    };
    let messages = vec![Message {
        id: MessageId("m1".into()),
        role: Role::User,
        timestamp: Utc::now(),
        content: vec![ContentBlock::Text("<b>bold & \"quoted\"</b>".into())],
        model: None,
        token_usage: None,
    }];

    let html = export::to_html(&session, &messages);

    assert!(
        !html.contains("<script>alert"),
        "should escape script tags in title"
    );
    assert!(
        html.contains("&lt;script&gt;"),
        "should HTML-escape angle brackets"
    );
    assert!(
        html.contains("&amp;"),
        "should escape ampersands in content"
    );
}

#[test]
fn html_escapes_language_attribute() {
    use aghist::model::*;
    use chrono::Utc;

    let session = Session {
        id: SessionId("test".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("test".into()),
        git_branch: None,
        started_at: Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from("/tmp/test"),
    };
    let messages = vec![Message {
        id: MessageId("m1".into()),
        role: Role::Assistant,
        timestamp: Utc::now(),
        content: vec![ContentBlock::CodeBlock {
            language: Some("rust\" onclick=\"alert(1)".into()),
            code: "fn main() {}".into(),
        }],
        model: None,
        token_usage: None,
    }];

    let html = export::to_html(&session, &messages);

    assert!(
        !html.contains(r#"" onclick="#),
        "should escape quotes in language attribute to prevent attribute breakout"
    );
    assert!(
        html.contains("&quot;"),
        "should HTML-escape quotes in language attribute"
    );
}
