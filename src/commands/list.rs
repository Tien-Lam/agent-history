use std::io::{self, Write as _};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK, EXIT_USAGE};
use aghist::dto::{CursorMeta, ListEnvelope, SessionRow};
use aghist::federated;
use aghist::model::{Provider, Session};
use aghist::output::OutputMode;
use aghist::{provider, query_scope};

use super::super::cli::FilterArgs;
use super::discovery::{federated_discovery_for_commands, source_for_session};
use super::filtering::{
    metadata_filter_matches_source, session_has_matching_message, session_matches,
};

pub(crate) fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    mode: OutputMode,
    limit: usize,
    cursor: Option<&str>,
    filters: &FilterArgs,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    let needs_messages = filters.role.is_some() || filters.has_tool_call;
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = discovery.source_by_session;
    let mut all_sessions: Vec<ListedSession> = discovery
        .sessions
        .into_iter()
        .filter(|s| session_matches(s, filters, project_needle.as_deref()))
        .filter(|s| {
            metadata_filter_matches_source(
                s,
                source_for_session(&source_by_session, s),
                metadata_keys,
            )
        })
        .filter(|s| !needs_messages || session_has_matching_message(providers, s, filters))
        .map(|session| {
            let source = source_by_session
                .get(session.identity_key().as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str)
                .to_string();
            ListedSession { source, session }
        })
        .collect();
    let provider_counts = (!mode.is_machine()).then(|| source_provider_counts(&all_sessions));

    // Canonical sort: started_at DESC, session_id ASC, identity key ASC. The
    // identity key keeps pagination total when local and remote sources reuse
    // a provider session id.
    all_sessions.sort_by(compare_listed_sessions);

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
            .position(|listed| {
                // Match the canonical order: started_at DESC, id ASC. We want
                // the first session strictly *after* the cursor key.
                listed_session_is_after_cursor(listed, c)
            })
            .unwrap_or(all_sessions.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(all_sessions.len());
    let page = &all_sessions[page_start..page_end];

    let next_cursor = if page_end < all_sessions.len() {
        page.last().map(|listed| {
            aghist::cursor::ListCursor {
                started_at: listed.session.started_at,
                session_id: listed.session.id.0.clone(),
                session_key: listed.session.identity_key(),
            }
            .encode()
        })
    } else {
        None
    };

    match mode {
        OutputMode::Human => {
            let provider_counts = provider_counts.unwrap_or_default();
            render_list_human(&provider_counts, page, total, next_cursor.as_deref()).map_err(
                |e| ErrorEnvelope::new("io-error", format!("failed to write list output: {e}")),
            )?;
        }
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

struct ListedSession {
    source: String,
    session: Session,
}

fn compare_listed_sessions(a: &ListedSession, b: &ListedSession) -> std::cmp::Ordering {
    b.session
        .started_at
        .cmp(&a.session.started_at)
        .then_with(|| a.session.id.0.cmp(&b.session.id.0))
        .then_with(|| a.session.identity_key().cmp(&b.session.identity_key()))
}

fn listed_session_is_after_cursor(
    listed: &ListedSession,
    cursor: &aghist::cursor::ListCursor,
) -> bool {
    if listed.session.started_at != cursor.started_at {
        return listed.session.started_at < cursor.started_at;
    }
    if listed.session.id.0 != cursor.session_id {
        return listed.session.id.0 > cursor.session_id;
    }

    if cursor.session_key.is_empty() {
        return false;
    }
    listed.session.identity_key().as_str() > cursor.session_key.as_str()
}

fn source_provider_counts(sessions: &[ListedSession]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for listed in sessions {
        let label = source_provider_label(&listed.source, listed.session.provider);
        if let Some((_, count)) = counts.iter_mut().find(|(existing, _)| existing == &label) {
            *count += 1;
        } else {
            counts.push((label, 1));
        }
    }
    counts
}

fn source_provider_label(source: &str, provider: Provider) -> String {
    if source == federated::LOCAL_SOURCE {
        provider.to_string()
    } else {
        format!("{source}/{provider}")
    }
}

fn render_list_human(
    provider_counts: &[(String, usize)],
    sessions: &[ListedSession],
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
    sessions: &[ListedSession],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let rows: Vec<SessionRow> = sessions.iter().map(session_row).collect();
    let doc = ListEnvelope {
        sessions: rows,
        meta: CursorMeta::new(total, next_cursor),
    };
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &doc).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_list_ndjson(
    sessions: &[ListedSession],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = session_row(s);
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

fn session_row(listed: &ListedSession) -> SessionRow {
    SessionRow::from_session(&listed.session, &listed.source)
}
