use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::services::lookup as lookup_service;
use aghist::{provider, query_scope};

use super::super::cli::ShowFormat;
use super::discovery::federated_discovery_for_commands;

mod render;

pub(crate) fn show_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    raw_ref: &str,
    format: ShowFormat,
    include_context: u32,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let target = lookup_service::load_citation_by_selector(
        providers,
        &discovery,
        raw_ref,
        include_context as usize,
    )?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        ShowFormat::Md => render::show_md(
            &mut out,
            &target.citation_ref,
            &target.session,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
        ShowFormat::Json => render::show_json(
            &mut out,
            &target.citation_ref,
            &target.citation,
            &target.session,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
        ShowFormat::Text => render::show_text(
            &mut out,
            &target.citation_ref,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
    }
    .map_err(|e| ErrorEnvelope::io("failed to write show output", e))?;

    Ok(EXIT_OK)
}
