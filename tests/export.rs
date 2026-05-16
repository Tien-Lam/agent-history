mod common;

use std::path::PathBuf;

use aghist::export::{self, ExportFormat};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use common::helpers::fixtures_dir;

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
    assert!(
        md.contains("## Assistant"),
        "should have assistant role header"
    );
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
        assert!(
            md.contains("<details>"),
            "tool calls should be in details tags"
        );
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
    assert!(
        md.contains("プロジェクト"),
        "markdown should preserve CJK project name"
    );
    assert!(
        md.contains("你好世界"),
        "markdown should preserve CJK content"
    );
    assert!(md.contains("🌍"), "markdown should preserve emoji");

    let json = export::to_json(&session, &messages);
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("JSON should be valid with unicode");
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

// ─── Notes (private annotations) ───────────────────────────────────────────────

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

#[test]
fn markdown_injects_session_and_turn_notes_at_citation_refs() {
    let (session, messages) = sample_session();
    let notes = vec![
        make_note(1, "claude-code/abc-123", "session-wide thought"),
        make_note(2, "claude-code/abc-123#2", "thought about turn 2"),
    ];
    let md = export::to_markdown_with_notes(&session, &messages, &notes);

    assert!(
        md.contains("Private annotations"),
        "session-level header present"
    );
    assert!(md.contains("session-wide thought"));
    assert!(md.contains("thought about turn 2"));

    // turn-2 note must appear AFTER the turn-2 body but BEFORE turn-3's role header.
    let turn2_pos = md.find("turn-2 body").expect("turn-2 body");
    let note_pos = md.find("thought about turn 2").expect("turn-2 note");
    let turn3_pos = md.find("turn-3 body").expect("turn-3 body");
    assert!(
        turn2_pos < note_pos && note_pos < turn3_pos,
        "note must sit between turn 2 and turn 3"
    );

    // Annotation marker is preserved so readers can't conflate notes with content.
    assert!(
        md.contains("Private annotation"),
        "every inlined note carries the private-annotation marker"
    );
}

#[test]
fn markdown_ignores_notes_for_other_sessions() {
    let (session, messages) = sample_session();
    let notes = vec![make_note(1, "claude-code/some-other-session#1", "not mine")];
    let md = export::to_markdown_with_notes(&session, &messages, &notes);
    assert!(!md.contains("not mine"));
    assert!(!md.contains("Private annotations"));
}

#[test]
fn json_emits_notes_array_marked_as_private_annotation() {
    let (session, messages) = sample_session();
    let notes = vec![
        make_note(7, "claude-code/abc-123", "session-wide"),
        make_note(8, "claude-code/abc-123#2", "turn-2"),
    ];
    let json_str = export::to_json_with_notes(&session, &messages, &notes);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");
    let notes_arr = parsed
        .get("notes")
        .expect("notes field")
        .as_array()
        .unwrap();
    assert_eq!(notes_arr.len(), 2);
    for n in notes_arr {
        assert_eq!(
            n.get("kind").and_then(|v| v.as_str()),
            Some("private-annotation")
        );
        assert!(n.get("body").is_some());
        assert!(n.get("session_ref").is_some());
    }
}

#[test]
fn json_matches_notes_against_explicit_source_qualified_session_ref() {
    let (session, messages) = sample_session();
    let notes = vec![
        make_note(7, "claude-code/abc-123#2", "local turn-2"),
        make_note(8, "laptop:claude-code/abc-123#2", "remote turn-2"),
    ];
    let json_str = export::export_with_notes_for_session_ref(
        ExportFormat::Json,
        &session,
        &messages,
        &notes,
        "laptop:claude-code/abc-123",
    );
    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");
    let notes_arr = parsed
        .get("notes")
        .expect("notes field")
        .as_array()
        .unwrap();
    assert_eq!(notes_arr.len(), 1);
    assert_eq!(notes_arr[0]["session_ref"], "laptop:claude-code/abc-123#2");
    assert_eq!(notes_arr[0]["body"], "remote turn-2");
}

#[test]
fn json_omits_notes_field_when_no_notes_match() {
    let (session, messages) = sample_session();
    let json_str = export::to_json_with_notes(&session, &messages, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(parsed.get("notes").is_none(), "no notes -> no field");
}

#[test]
fn html_inlines_notes_with_private_annotation_marker() {
    let (session, messages) = sample_session();
    let notes = vec![
        make_note(1, "claude-code/abc-123", "session note"),
        make_note(2, "claude-code/abc-123#1", "turn-1 note"),
    ];
    let html = export::to_html_with_notes(&session, &messages, &notes);
    assert!(
        html.contains("session-notes"),
        "session-level section rendered"
    );
    assert!(html.contains("data-kind=\"private-annotation\""));
    assert!(html.contains("Private annotation"));
    assert!(html.contains("session note"));
    assert!(html.contains("turn-1 note"));
}

#[test]
fn html_escapes_note_body_to_prevent_xss() {
    let (session, messages) = sample_session();
    let notes = vec![make_note(
        1,
        "claude-code/abc-123#1",
        "<script>alert('xss')</script>",
    )];
    let html = export::to_html_with_notes(&session, &messages, &notes);
    assert!(
        !html.contains("<script>alert"),
        "note body must not break out of escaping"
    );
    assert!(html.contains("&lt;script&gt;"));
}

#[test]
fn export_no_notes_matches_legacy_output() {
    let (session, messages) = sample_session();
    let with_empty = export::export_with_notes(ExportFormat::Markdown, &session, &messages, &[]);
    let legacy = export::export(ExportFormat::Markdown, &session, &messages);
    assert_eq!(with_empty, legacy);
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
