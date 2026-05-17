use std::io::{self, Write as _};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK, EXIT_USAGE};
use aghist::dto::{CursorMeta, ListEnvelope, SessionRow};
use aghist::model::Provider;
use aghist::output::{write_json_line, OutputMode};
use aghist::services::list as list_service;
use aghist::{provider, query_scope};

use super::super::cli::FilterArgs;
use super::discovery::federated_discovery_for_commands;

pub(crate) fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    mode: OutputMode,
    limit: usize,
    cursor: Option<&str>,
    filters: &FilterArgs,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let search_filters = filters.to_search_filters();
    let page = match list_service::list_sessions_page(
        providers,
        discovery,
        list_service::ListSessionsRequest {
            limit,
            cursor,
            filters: &search_filters,
            metadata_keys,
        },
    ) {
        Ok(page) => page,
        Err(list_service::ListSessionsError::InvalidCursor) => {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    };
    let provider_counts = (!mode.is_machine()).then_some(page.provider_counts.as_slice());

    match mode {
        OutputMode::Human => {
            let provider_counts = provider_counts.unwrap_or_default();
            render_list_human(
                provider_counts,
                &page.sessions,
                page.total,
                page.next_cursor.as_deref(),
            )
            .map_err(|e| ErrorEnvelope::io("failed to write list output", e))?;
        }
        OutputMode::Json => {
            render_list_json(&page.sessions, page.total, page.next_cursor.as_deref())
                .map_err(|e| ErrorEnvelope::io("failed to write JSON output", e))?;
        }
        OutputMode::Ndjson => {
            render_list_ndjson(&page.sessions, page.total, page.next_cursor.as_deref())
                .map_err(|e| ErrorEnvelope::io("failed to write NDJSON output", e))?;
        }
    }

    if page.total == 0 {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn source_provider_label(source: &str, provider: Provider) -> String {
    list_service::source_provider_label(source, provider)
}

fn render_list_human(
    provider_counts: &[(String, usize)],
    sessions: &[list_service::ListedSession],
    total: usize,
    next_cursor: Option<&str>,
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    for (label, count) in provider_counts {
        writeln!(out, "{label}: {count} sessions")?;
    }
    writeln!(out, "\nTotal: {total} sessions\n")?;
    for listed in sessions {
        let s = &listed.session;
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
        let provider = source_provider_label(&listed.source, s.provider);
        writeln!(
            out,
            "  {} | {} | {} | {} | {}",
            s.started_at.format("%Y-%m-%d %H:%M"),
            provider,
            project,
            branch,
            summary
        )?;
    }
    if let Some(token) = next_cursor {
        writeln!(
            out,
            "\n(more results — pass --cursor {token} for the next page)"
        )?;
    }
    Ok(())
}

fn render_list_json(
    sessions: &[list_service::ListedSession],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let rows: Vec<SessionRow> = sessions.iter().map(session_row).collect();
    let doc = ListEnvelope {
        sessions: rows,
        meta: CursorMeta::new(total, next_cursor),
    };
    let mut out = std::io::stdout().lock();
    write_json_line(&mut out, &doc)
}

fn render_list_ndjson(
    sessions: &[list_service::ListedSession],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = session_row(s);
        write_json_line(&mut out, &row)?;
    }
    // Trailing meta record terminates the stream so consumers can detect EOF
    // without watching stdin close. Keyed by `meta` so it never collides with
    // a session row (which is keyed by `id`).
    let meta = serde_json::json!({
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    write_json_line(&mut out, &meta)
}

fn session_row(listed: &list_service::ListedSession) -> SessionRow {
    SessionRow::from_session(&listed.session, &listed.source)
}
