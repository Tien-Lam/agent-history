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
mod tests;
