use std::io::{self, Read};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_USAGE};

pub(super) fn resolve_search_query(
    query: Option<&str>,
    query_file: Option<&Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    let mut sources = 0;
    if query.is_some() {
        sources += 1;
    }
    if query_file.is_some() {
        sources += 1;
    }
    if stdin {
        sources += 1;
    }
    if sources == 0 {
        return Err(ErrorEnvelope::new(
            "usage",
            "search requires a query (positional, --query-file, or --stdin)",
        )
        .with_hint("Run `aghist search --help` for usage."));
    }

    if let Some(q) = query {
        return Ok(q.to_string());
    }

    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
        })?;
    } else if let Some(path) = query_file {
        if path == Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read query file {}: {e}", path.display()),
                )
            })?;
        }
    }

    Ok(buf.trim_end().to_string())
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
