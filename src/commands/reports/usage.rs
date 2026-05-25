use std::collections::HashSet;
use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::output::should_emit_json;
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;

use super::collect::collect_federated_filtered_sessions;
use super::output::{render_usage_human, render_usage_json};

pub(crate) fn usage_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    group_by: aghist::usage::GroupBy,
    limit: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let sessions = collect_federated_filtered_sessions(providers, scope, filters, metadata_keys);

    let report = aghist::usage::aggregate(&sessions, group_by);
    if report.rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let total_rows = report.rows.len();
    let trimmed = if total_rows > limit {
        let mut r = report;
        r.rows.truncate(limit);
        r
    } else {
        report
    };

    let want_json = should_emit_json(force_json);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_usage_json(&mut out, &trimmed, group_by, total_rows)
    } else {
        render_usage_human(&mut out, &trimmed, group_by, total_rows)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write usage output", e))?;

    Ok(EXIT_OK)
}
