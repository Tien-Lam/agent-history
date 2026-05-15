use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK, EXIT_USAGE};
use aghist::model::Session;
use aghist::output::OutputMode;
use aghist::provider;

use super::super::cli::FilterArgs;
use super::filtering::{metadata_filter_matches, session_has_matching_message, session_matches};

pub(crate) fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
    limit: usize,
    cursor: Option<&str>,
    filters: &FilterArgs,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();

    let needs_messages = filters.role.is_some() || filters.has_tool_call;
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    for p in providers {
        // When --provider is set, skip non-matching providers entirely so we
        // don't pay discovery cost for sessions we'd just throw away.
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(sessions) => {
                let kept: Vec<Session> = sessions
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref()))
                    .filter(|s| metadata_filter_matches(s, metadata_keys))
                    .filter(|s| {
                        !needs_messages || session_has_matching_message(p.as_ref(), s, filters)
                    })
                    .collect();
                if !mode.is_machine() {
                    println!("{}: {} sessions", p.provider(), kept.len());
                }
                all_sessions.extend(kept);
            }
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
            }
        }
    }

    // Canonical sort: started_at DESC, session_id ASC. The id tie-break makes
    // the cursor's keyset comparison total even when two sessions share a
    // millisecond timestamp.
    all_sessions.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    let total = all_sessions.len();

    let after = if let Some(token) = cursor {
        if let Ok(c) = aghist::cursor::ListCursor::decode(token) {
            Some(c)
        } else {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    } else {
        None
    };

    let page_start = match &after {
        Some(c) => all_sessions
            .iter()
            .position(|s| {
                // Match the canonical order: started_at DESC, id ASC. We want
                // the first session strictly *after* the cursor key.
                s.started_at < c.started_at
                    || (s.started_at == c.started_at && s.id.0 > c.session_id)
            })
            .unwrap_or(all_sessions.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(all_sessions.len());
    let page = &all_sessions[page_start..page_end];

    let next_cursor = if page_end < all_sessions.len() {
        page.last().map(|s| {
            aghist::cursor::ListCursor {
                started_at: s.started_at,
                session_id: s.id.0.clone(),
            }
            .encode()
        })
    } else {
        None
    };

    match mode {
        OutputMode::Human => render_list_human(page, total, next_cursor.as_deref()),
        OutputMode::Json => render_list_json(page, total, next_cursor.as_deref()).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?,
        OutputMode::Ndjson => {
            render_list_ndjson(page, total, next_cursor.as_deref()).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to write NDJSON output: {e}"))
            })?;
        }
    }

    if all_sessions.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn render_list_human(sessions: &[Session], total: usize, next_cursor: Option<&str>) {
    println!("\nTotal: {total} sessions\n");
    for s in sessions {
        let project = s.project_name.as_deref().unwrap_or("(unknown)");
        let branch = s.git_branch.as_deref().unwrap_or("");
        let summary = match s.summary.as_deref() {
            Some(text) if text.chars().count() > 60 => {
                let mut s: String = text.chars().take(57).collect();
                s.push_str("...");
                s
            }
            Some(text) => text.to_string(),
            None => String::new(),
        };
        println!(
            "  {} | {} | {} | {} | {}",
            s.started_at.format("%Y-%m-%d %H:%M"),
            s.provider,
            project,
            branch,
            summary
        );
    }
    if let Some(token) = next_cursor {
        println!("\n(more results — pass --cursor {token} for the next page)");
    }
}

#[derive(serde::Serialize)]
struct SessionRow<'a> {
    id: &'a str,
    provider: aghist::model::Provider,
    project: Option<&'a str>,
    branch: Option<&'a str>,
    summary: Option<&'a str>,
    started_at: chrono::DateTime<chrono::Utc>,
    message_count: usize,
}

impl<'a> SessionRow<'a> {
    fn from_session(s: &'a Session) -> Self {
        Self {
            id: s.id.0.as_str(),
            provider: s.provider,
            project: s.project_name.as_deref(),
            branch: s.git_branch.as_deref(),
            summary: s.summary.as_deref(),
            started_at: s.started_at,
            message_count: s.message_count,
        }
    }
}

fn render_list_json(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let rows: Vec<SessionRow<'_>> = sessions.iter().map(SessionRow::from_session).collect();
    let doc = serde_json::json!({
        "sessions": rows,
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &doc).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_list_ndjson(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = SessionRow::from_session(s);
        serde_json::to_writer(&mut out, &row).map_err(std::io::Error::other)?;
        writeln!(out)?;
    }
    // Trailing meta record terminates the stream so consumers can detect EOF
    // without watching stdin close. Keyed by `meta` so it never collides with
    // a session row (which is keyed by `id`).
    let meta = serde_json::json!({
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    serde_json::to_writer(&mut out, &meta).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}
