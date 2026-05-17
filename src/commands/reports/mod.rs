use std::collections::HashSet;
use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK, EXIT_USAGE};
use aghist::{provider, query_scope};
use chrono::Utc;

use super::super::cli::FilterArgs;
use super::discovery::{qualified_citation_ref, qualified_session_ref, source_for_session};

mod collect;
mod output;

use collect::{
    collect_federated_filtered_sessions, collect_federated_message_bundles,
    normalized_project_filter,
};
use output::{
    render_project_human, render_project_json, render_report, render_usage_human, render_usage_json,
};

pub(crate) fn usage_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    group_by: aghist::usage::GroupBy,
    limit: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let project_needle = normalized_project_filter(filters);
    let sessions = collect_federated_filtered_sessions(
        providers,
        scope,
        filters,
        project_needle.as_deref(),
        metadata_keys,
    );

    let report = aghist::usage::aggregate(&sessions, group_by);
    if report.rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let total_rows = report.rows.len();
    let trimmed = if limit > 0 && total_rows > limit {
        let mut r = report;
        r.rows.truncate(limit);
        r
    } else {
        report
    };

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_usage_json(&mut out, &trimmed, group_by, total_rows)
    } else {
        render_usage_human(&mut out, &trimmed, group_by, total_rows)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write usage output: {e}")))?;

    Ok(EXIT_OK)
}

pub(crate) fn project_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    name: &str,
    limits: aghist::project::ProjectLimits,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let needle = name.trim();
    if needle.is_empty() {
        ErrorEnvelope::new("usage", "project <name> must not be empty").emit();
        return Ok(EXIT_USAGE);
    }
    let needle_lower = needle.to_lowercase();
    let extra_project = normalized_project_filter(filters);
    let collected = collect_federated_message_bundles(
        providers,
        scope,
        filters,
        extra_project.as_deref(),
        metadata_keys,
        |session| {
            let project_name = session.project_name.as_deref().unwrap_or("");
            project_name.to_lowercase().contains(&needle_lower)
        },
    );
    let bundles = collected.bundles;

    if bundles.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let report = aghist::project::aggregate_with_refs(
        needle,
        &bundles,
        limits,
        |session| source_for_session(&collected.source_by_session, session).to_string(),
        |session| qualified_session_ref(&collected.source_by_session, session),
        |session, turn| qualified_citation_ref(&collected.source_by_session, session, turn),
    );
    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_project_json(&mut out, &report)
    } else {
        render_project_human(&mut out, &report)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write project output: {e}")))?;

    Ok(EXIT_OK)
}

pub(crate) fn report_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    window_days: i64,
    limits: aghist::report::ReportLimits,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let now = Utc::now();
    let end = filters.until.unwrap_or(now);
    let start = filters
        .since
        .unwrap_or_else(|| end - chrono::Duration::days(window_days.max(1)));
    if end < start {
        return Err(ErrorEnvelope::new(
            "usage",
            "--until must be greater than or equal to --since",
        )
        .with_hint("Pass timestamps in chronological order, or rely on --days."));
    }
    let window = aghist::report::ReportWindow::between(start, end);

    let project_needle = normalized_project_filter(filters);
    let collected = collect_federated_message_bundles(
        providers,
        scope,
        filters,
        project_needle.as_deref(),
        metadata_keys,
        |session| session.started_at >= start && session.started_at <= end,
    );
    let bundles = collected.bundles;

    if bundles.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let envelope = aghist::report::aggregate_with_refs(
        window,
        &bundles,
        limits,
        |session| source_for_session(&collected.source_by_session, session).to_string(),
        |session| qualified_session_ref(&collected.source_by_session, session),
        |session, turn| qualified_citation_ref(&collected.source_by_session, session, turn),
    );
    let stdout = io::stdout();
    let mut out = stdout.lock();
    render_report(&mut out, &envelope, force_json)?;
    Ok(EXIT_OK)
}
