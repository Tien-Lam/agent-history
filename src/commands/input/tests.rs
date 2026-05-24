use super::*;

fn messages() -> TextInputMessages<'static> {
    TextInputMessages {
        missing: "missing input",
        multiple: "multiple inputs",
        stdin_read: "failed to read stdin",
        file_read_prefix: "failed to read file",
        usage_hint: Some("usage hint"),
    }
}

#[test]
fn inline_input_wins_without_trimming() {
    let text = read_text_input_inner(
        TextInput {
            inline: Some("  query\n"),
            file: None,
            stdin: false,
        },
        messages(),
        true,
        None,
    )
    .unwrap();

    assert_eq!(text, "  query\n");
}

#[test]
fn file_input_can_trim_trailing_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("query.txt");
    std::fs::write(&path, "query\n\n").unwrap();

    let text = read_text_input_inner(
        TextInput {
            inline: None,
            file: Some(&path),
            stdin: false,
        },
        messages(),
        true,
        None,
    )
    .unwrap();

    assert_eq!(text, "query");
}

#[test]
fn file_input_can_preserve_body_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("body.txt");
    std::fs::write(&path, "body\n").unwrap();

    let text = read_text_input_inner(
        TextInput {
            inline: None,
            file: Some(&path),
            stdin: false,
        },
        messages(),
        false,
        None,
    )
    .unwrap();

    assert_eq!(text, "body\n");
}

#[test]
fn missing_and_multiple_inputs_are_usage_errors() {
    let missing = read_text_input_inner(
        TextInput {
            inline: None,
            file: None,
            stdin: false,
        },
        messages(),
        false,
        None,
    )
    .unwrap_err();
    assert_eq!(missing.kind, "usage");
    assert_eq!(missing.message, "missing input");
    assert_eq!(missing.hint.as_deref(), Some("usage hint"));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("body.txt");
    let multiple = read_text_input_inner(
        TextInput {
            inline: Some("body"),
            file: Some(&path),
            stdin: false,
        },
        messages(),
        false,
        None,
    )
    .unwrap_err();
    assert_eq!(multiple.kind, "usage");
    assert_eq!(multiple.message, "multiple inputs");
}

#[test]
fn limited_input_rejects_oversized_inline_value() {
    let err = read_text_input_with_limit(
        TextInput {
            inline: Some("abcdef"),
            file: None,
            stdin: false,
        },
        messages(),
        false,
        5,
        "test input",
    )
    .unwrap_err();

    assert_eq!(err.kind, "usage");
    assert_eq!(err.message, "test input exceeds 5 byte limit");
}

#[test]
fn limited_input_rejects_oversized_file_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("body.txt");
    std::fs::write(&path, "abcdef").unwrap();

    let err = read_text_input_with_limit(
        TextInput {
            inline: None,
            file: Some(&path),
            stdin: false,
        },
        messages(),
        false,
        5,
        "test input",
    )
    .unwrap_err();

    assert_eq!(err.kind, "usage");
    assert_eq!(err.message, "test input exceeds 5 byte limit");
}

#[test]
fn limited_input_accepts_exact_byte_limit() {
    let text = read_text_input_with_limit(
        TextInput {
            inline: Some("abcde"),
            file: None,
            stdin: false,
        },
        messages(),
        false,
        5,
        "test input",
    )
    .unwrap();

    assert_eq!(text, "abcde");
}
