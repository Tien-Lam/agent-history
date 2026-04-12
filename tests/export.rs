mod common;

use std::path::PathBuf;

use aghist::export::{self, ExportFormat};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use common::helpers::fixtures_dir;

fn load_fixture_session() -> (aghist::model::Session, Vec<aghist::model::Message>) {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let session = sessions.into_iter().next().expect("fixture has at least one session");
    let messages = provider.load_messages(&session).unwrap();
    (session, messages)
}

// ─── Markdown ──────────────────────────────────────────────────────────────────

#[test]
fn markdown_has_header_and_metadata() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.starts_with("# "), "should start with H1 header");
    assert!(md.contains("**Provider**"), "should have provider metadata");
    assert!(md.contains("**Date**"), "should have date metadata");
    assert!(md.contains("---"), "should have horizontal rule separator");
}

#[test]
fn markdown_has_role_headers() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.contains("## You"), "should have user role header");
    assert!(md.contains("## Assistant"), "should have assistant role header");
}

#[test]
fn markdown_preserves_code_blocks() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.contains("```"), "should contain fenced code blocks");
}

#[test]
fn markdown_has_tool_call_sections() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    let has_tool_use = messages
        .iter()
        .flat_map(|m| &m.content)
        .any(|c| matches!(c, aghist::model::ContentBlock::ToolUse(_)));

    if has_tool_use {
        assert!(md.contains("<details>"), "tool calls should be in details tags");
        assert!(md.contains("Tool:"), "tool call should show tool name");
    }
}

// ─── JSON ──────────────────────────────────────────────────────────────────────

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
    assert!(sess.get("provider").is_some(), "session should have provider");
    assert!(sess.get("started_at").is_some(), "session should have started_at");
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
        assert!(msg.get("content").is_some(), "each message should have content");
        assert!(msg.get("timestamp").is_some(), "each message should have timestamp");
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

// ─── HTML ──────────────────────────────────────────────────────────────────────

#[test]
fn html_is_self_contained() {
    let (session, messages) = load_fixture_session();
    let html = export::to_html(&session, &messages);

    assert!(html.contains("<!DOCTYPE html>"), "should have doctype");
    assert!(html.contains("<style>"), "should have inline CSS");
    assert!(html.contains("</html>"), "should be closed HTML");
    // Self-contained means no external stylesheet or script links
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

    assert!(html.contains("class=\"message user\""), "should have user messages");
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

// ─── HTML language attribute injection ──────────────────────────────────────────

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

    // The quote in the language is escaped, so it can't break out of the attribute
    assert!(
        !html.contains(r#"" onclick="#),
        "should escape quotes in language attribute to prevent attribute breakout"
    );
    assert!(
        html.contains("&quot;"),
        "should HTML-escape quotes in language attribute"
    );
}

// ─── UTF-8 export ──────────────────────────────────────────────────────────────

#[test]
fn export_handles_unicode_content() {
    use aghist::model::*;
    use chrono::Utc;

    let session = Session {
        id: SessionId("unicode-test".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("プロジェクト".into()),
        git_branch: Some("feature/日本語".into()),
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
        content: vec![
            ContentBlock::Text("你好世界 🌍 مرحبا".into()),
            ContentBlock::CodeBlock {
                language: Some("python".into()),
                code: "print('café ☕')".into(),
            },
        ],
        model: None,
        token_usage: None,
    }];

    let md = export::to_markdown(&session, &messages);
    assert!(md.contains("プロジェクト"), "markdown should preserve CJK project name");
    assert!(md.contains("你好世界"), "markdown should preserve CJK content");
    assert!(md.contains("🌍"), "markdown should preserve emoji");

    let json = export::to_json(&session, &messages);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid with unicode");
    assert_eq!(parsed["session"]["project_name"], "プロジェクト");

    let html = export::to_html(&session, &messages);
    assert!(html.contains("プロジェクト"), "HTML should preserve CJK");
    assert!(html.contains("مرحبا"), "HTML should preserve RTL text");
}

// ─── ExportFormat ──────────────────────────────────────────────────────────────

#[test]
fn format_from_str_roundtrip() {
    for name in &["md", "markdown", "json", "html"] {
        let fmt: ExportFormat = name.parse().unwrap();
        assert!(!fmt.label().is_empty());
        assert!(!fmt.extension().is_empty());
    }
}

#[test]
fn format_from_str_invalid() {
    assert!("pdf".parse::<ExportFormat>().is_err());
    assert!("txt".parse::<ExportFormat>().is_err());
}

#[test]
fn export_dispatch_matches_format() {
    let (session, messages) = load_fixture_session();

    let md = export::export(ExportFormat::Markdown, &session, &messages);
    assert!(md.starts_with("# "), "Markdown dispatch");

    let json = export::export(ExportFormat::Json, &session, &messages);
    assert!(json.starts_with('{'), "JSON dispatch");

    let html = export::export(ExportFormat::Html, &session, &messages);
    assert!(html.contains("<!DOCTYPE html>"), "HTML dispatch");
}
