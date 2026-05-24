use std::fs::File;
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

pub(crate) fn read_text_input_with_limit(
    input: TextInput<'_>,
    messages: TextInputMessages<'_>,
    trim_read_trailing_newline: bool,
    max_bytes: usize,
    input_label: &str,
) -> Result<String, ErrorEnvelope> {
    read_text_input_inner(
        input,
        messages,
        trim_read_trailing_newline,
        Some((max_bytes, input_label)),
    )
}

fn read_text_input_inner(
    input: TextInput<'_>,
    messages: TextInputMessages<'_>,
    trim_read_trailing_newline: bool,
    limit: Option<(usize, &str)>,
) -> Result<String, ErrorEnvelope> {
    match source_count(&input) {
        0 => return Err(usage_error(messages.missing, messages.usage_hint)),
        1 => {}
        _ => return Err(usage_error(messages.multiple, messages.usage_hint)),
    }

    if let Some(value) = input.inline {
        enforce_limit(value.len(), limit)?;
        return Ok(value.to_string());
    }

    let buf = if input.stdin {
        read_limited_utf8(io::stdin().lock(), messages.stdin_read, limit)?
    } else if let Some(path) = input.file {
        if path == Path::new("-") {
            read_limited_utf8(io::stdin().lock(), messages.stdin_read, limit)?
        } else {
            let action = format!("{} {}", messages.file_read_prefix, path.display());
            let file = File::open(path).map_err(|e| ErrorEnvelope::io(&action, e))?;
            read_limited_utf8(file, action, limit)?
        }
    } else {
        String::new()
    };

    if trim_read_trailing_newline {
        Ok(buf.trim_end().to_string())
    } else {
        Ok(buf)
    }
}

fn read_limited_utf8<R: Read>(
    reader: R,
    action: impl AsRef<str>,
    limit: Option<(usize, &str)>,
) -> Result<String, ErrorEnvelope> {
    let mut reader = reader;
    if let Some((max_bytes, _)) = limit {
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take(max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| ErrorEnvelope::io(action.as_ref(), e))?;
        enforce_limit(bytes.len(), limit)?;
        return String::from_utf8(bytes).map_err(|e| ErrorEnvelope::io(action.as_ref(), e));
    }

    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .map_err(|e| ErrorEnvelope::io(action.as_ref(), e))?;
    Ok(buf)
}

fn enforce_limit(bytes: usize, limit: Option<(usize, &str)>) -> Result<(), ErrorEnvelope> {
    let Some((max_bytes, input_label)) = limit else {
        return Ok(());
    };
    if bytes <= max_bytes {
        return Ok(());
    }
    Err(ErrorEnvelope::new(
        "usage",
        format!("{input_label} exceeds {max_bytes} byte limit"),
    ))
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
