use aghist::export::{self, ExportFormat};

use super::{make_note, sample_session};

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

    let turn2_pos = md.find("turn-2 body").expect("turn-2 body");
    let note_pos = md.find("thought about turn 2").expect("turn-2 note");
    let turn3_pos = md.find("turn-3 body").expect("turn-3 body");
    assert!(
        turn2_pos < note_pos && note_pos < turn3_pos,
        "note must sit between turn 2 and turn 3"
    );

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
