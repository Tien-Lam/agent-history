use super::*;

#[test]
fn top_files_counts_tool_call_paths() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Read", r#"{"file_path":"src/main.rs"}"#, ts(2025, 9)),
        tool_call("Edit", r#"{"file_path":"src/main.rs"}"#, ts(2025, 10)),
        tool_call("Read", r#"{"file_path":"src/lib.rs"}"#, ts(2025, 11)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert_eq!(report.top_files.len(), 2);
    assert_eq!(report.top_files[0].path, "src/main.rs");
    assert_eq!(report.top_files[0].count, 2);
    assert_eq!(report.top_files[1].path, "src/lib.rs");
    assert_eq!(report.top_files[1].count, 1);
    assert_eq!(report.meta.files_total, 2);
}

#[test]
fn top_files_skips_non_path_args_and_unparseable_json() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Bash", r#"{"command":"ls -la"}"#, ts(2025, 9)),
        tool_call("Weird", "not-valid-json", ts(2025, 10)),
        tool_call("Read", r#"{"file_path":""}"#, ts(2025, 11)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(report.top_files.is_empty());
}

#[test]
fn limits_truncate_sections_but_meta_preserves_totals() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Read", r#"{"file_path":"a"}"#, ts(2025, 9)),
        tool_call("Read", r#"{"file_path":"b"}"#, ts(2025, 10)),
        tool_call("Read", r#"{"file_path":"c"}"#, ts(2025, 11)),
    ];
    let limits = ProjectLimits {
        decisions: 5,
        todos: 5,
        threads: 5,
        files: 2,
    };
    let report = aggregate("alpha", &[(s, m)], limits);
    assert_eq!(report.top_files.len(), 2);
    assert_eq!(report.meta.files_total, 3);
}
