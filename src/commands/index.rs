use aghist::cli_error::{ErrorEnvelope, EXIT_ERROR, EXIT_OK};
use aghist::model::Provider;
use aghist::output::write_json_line;
use aghist::services::index::{self as index_service, IndexSummary};
use aghist::{provider, query_scope};

pub(crate) fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let summary =
        index_service::build_index_summary(providers, scope, filter, force, accept_download)?;
    let exit_code = if summary.has_errors() {
        EXIT_ERROR
    } else {
        EXIT_OK
    };
    write_index_summary(&summary)?;
    Ok(exit_code)
}

fn write_index_summary(summary: &IndexSummary) -> Result<(), ErrorEnvelope> {
    let mut out = std::io::stdout().lock();
    write_json_line(&mut out, summary)
        .map_err(|e| ErrorEnvelope::io("failed to write index output", e))
}
