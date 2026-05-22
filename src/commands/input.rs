use std::io::{self, Read};
use std::path::Path;

use aghist::cli_error::ErrorEnvelope;

#[derive(Clone, Copy)]
pub(crate) struct TextInput<'a> {
    pub(crate) inline: Option<&'a str>,
    pub(crate) file: Option<&'a Path>,
    pub(crate) stdin: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct TextInputMessages<'a> {
    pub(crate) missing: &'a str,
    pub(crate) multiple: &'a str,
    pub(crate) stdin_read: &'a str,
    pub(crate) file_read_prefix: &'a str,
    pub(crate) usage_hint: Option<&'a str>,
}

pub(crate) fn read_text_input(
    input: TextInput<'_>,
    messages: TextInputMessages<'_>,
    trim_read_trailing_newline: bool,
) -> Result<String, ErrorEnvelope> {
    match source_count(&input) {
        0 => return Err(usage_error(messages.missing, messages.usage_hint)),
        1 => {}
        _ => return Err(usage_error(messages.multiple, messages.usage_hint)),
    }

    if let Some(value) = input.inline {
        return Ok(value.to_string());
    }

    let mut buf = String::new();
    if input.stdin {
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| ErrorEnvelope::io(messages.stdin_read, e))?;
    } else if let Some(path) = input.file {
        if path == Path::new("-") {
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| ErrorEnvelope::io(messages.stdin_read, e))?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::io(
                    format!("{} {}", messages.file_read_prefix, path.display()),
                    e,
                )
            })?;
        }
    }

    if trim_read_trailing_newline {
        Ok(buf.trim_end().to_string())
    } else {
        Ok(buf)
    }
}

fn source_count(input: &TextInput<'_>) -> usize {
    usize::from(input.inline.is_some())
        + usize::from(input.file.is_some())
        + usize::from(input.stdin)
}

fn usage_error(message: &str, hint: Option<&str>) -> ErrorEnvelope {
    let envelope = ErrorEnvelope::new("usage", message);
    if let Some(hint) = hint {
        envelope.with_hint(hint)
    } else {
        envelope
    }
}

#[cfg(test)]
mod tests {
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
        let text = read_text_input(
            TextInput {
                inline: Some("  query\n"),
                file: None,
                stdin: false,
            },
            messages(),
            true,
        )
        .unwrap();

        assert_eq!(text, "  query\n");
    }

    #[test]
    fn file_input_can_trim_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("query.txt");
        std::fs::write(&path, "query\n\n").unwrap();

        let text = read_text_input(
            TextInput {
                inline: None,
                file: Some(&path),
                stdin: false,
            },
            messages(),
            true,
        )
        .unwrap();

        assert_eq!(text, "query");
    }

    #[test]
    fn file_input_can_preserve_body_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.txt");
        std::fs::write(&path, "body\n").unwrap();

        let text = read_text_input(
            TextInput {
                inline: None,
                file: Some(&path),
                stdin: false,
            },
            messages(),
            false,
        )
        .unwrap();

        assert_eq!(text, "body\n");
    }

    #[test]
    fn missing_and_multiple_inputs_are_usage_errors() {
        let missing = read_text_input(
            TextInput {
                inline: None,
                file: None,
                stdin: false,
            },
            messages(),
            false,
        )
        .unwrap_err();
        assert_eq!(missing.kind, "usage");
        assert_eq!(missing.message, "missing input");
        assert_eq!(missing.hint.as_deref(), Some("usage hint"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.txt");
        let multiple = read_text_input(
            TextInput {
                inline: Some("body"),
                file: Some(&path),
                stdin: false,
            },
            messages(),
            false,
        )
        .unwrap_err();
        assert_eq!(multiple.kind, "usage");
        assert_eq!(multiple.message, "multiple inputs");
    }
}
