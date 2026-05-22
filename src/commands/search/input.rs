use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_USAGE};

use crate::commands::input::{read_text_input, TextInput, TextInputMessages};

pub(super) fn resolve_search_query(
    query: Option<&str>,
    query_file: Option<&Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    read_text_input(
        TextInput {
            inline: query,
            file: query_file,
            stdin,
        },
        TextInputMessages {
            missing: "search requires a query (positional, --query-file, or --stdin)",
            multiple: "search accepts only one query source",
            stdin_read: "failed to read query from stdin",
            file_read_prefix: "failed to read query file",
            usage_hint: Some("Run `aghist search --help` for usage."),
        },
        true,
    )
}

pub(super) fn resolve_nonempty_search_query(
    query: Option<&str>,
    query_file: Option<&Path>,
    stdin: bool,
) -> Result<String, i32> {
    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Err(EXIT_USAGE);
        }
    };
    if resolved.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Err(EXIT_USAGE);
    }
    Ok(resolved)
}

pub(super) fn decode_search_cursor(
    cursor: Option<&str>,
) -> Result<Option<aghist::cursor::SearchCursor>, i32> {
    let Some(token) = cursor else {
        return Ok(None);
    };
    if let Ok(cursor) = aghist::cursor::SearchCursor::decode(token) {
        Ok(Some(cursor))
    } else {
        ErrorEnvelope::new("usage", "invalid --cursor token")
            .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
            .emit();
        Err(EXIT_USAGE)
    }
}
