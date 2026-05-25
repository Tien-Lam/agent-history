use std::collections::HashSet;
use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::{provider, query_scope};
use chrono::Utc;

use crate::cli::FilterArgs;

use super::collect::collect_federated_message_bundles;
use super::output::render_report;
use crate::commands::discovery::{
    qualified_citation_ref, qualified_session_ref, source_for_session,
};

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

    let collected =
        collect_federated_message_bundles(providers, scope, filters, metadata_keys, |session| {
            session.started_at >= start && session.started_at <= end
        });
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
