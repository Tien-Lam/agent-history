use std::collections::HashSet;
use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;

use super::collect::collect_federated_message_bundles;
use super::output::{render_project_human, render_project_json};
use crate::commands::discovery::{
    qualified_citation_ref, qualified_session_ref, source_for_session,
};

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
        return Err(ErrorEnvelope::new(
            "usage",
            "project <name> must not be empty",
        ));
    }
    let needle_lower = needle.to_lowercase();
    let collected =
        collect_federated_message_bundles(providers, scope, filters, metadata_keys, |session| {
            let project_name = session.project_name.as_deref().unwrap_or("");
            project_name.to_lowercase().contains(&needle_lower)
        });
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
    .map_err(|e| ErrorEnvelope::io("failed to write project output", e))?;

    Ok(EXIT_OK)
}
