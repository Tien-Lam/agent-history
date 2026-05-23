use super::*;

#[test]
fn collapsed_summary_prefers_file_path() {
    let s = collapsed_arg_summary(r#"{"file_path":"src/main.rs","limit":50}"#).unwrap();
    assert_eq!(s, "src/main.rs");
}

#[test]
fn collapsed_summary_uses_command_for_shell() {
    let s = collapsed_arg_summary(r#"{"command":"ls -la","description":"List files"}"#).unwrap();
    assert_eq!(s, "ls -la");
}

#[test]
fn collapsed_summary_truncates_long_values() {
    let long = "a".repeat(200);
    let json = format!(r#"{{"path":"{long}"}}"#);
    let s = collapsed_arg_summary(&json).unwrap();
    assert!(s.ends_with('\u{2026}'));
    assert!(s.chars().count() <= COLLAPSED_ARG_CHARS + 1);
}

#[test]
fn collapsed_summary_falls_back_to_first_line_for_non_json() {
    let s = collapsed_arg_summary("first line\nsecond line").unwrap();
    assert_eq!(s, "first line");
}

#[test]
fn collapsed_summary_returns_none_for_empty() {
    assert!(collapsed_arg_summary("").is_none());
    assert!(collapsed_arg_summary("   \n  ").is_none());
}

#[test]
fn collapsed_summary_skips_object_only_args() {
    assert!(collapsed_arg_summary(r#"{"nested":{"k":1}}"#).is_none());
}
