use aghist::cli_error::ErrorEnvelope;
use aghist::provider;

use super::super::cli::{
    resolve_export_args, resolve_index_args, resolve_show_args, Command, FilterArgs,
};
use super::diff::diff_command;
use super::export::export_session;
use super::index::run_index;
use super::search_dispatch::{dispatch_search_command, SearchDispatchArgs, SearchDispatchMode};
use super::show::show_command;

pub(crate) fn dispatch_lookup_command(
    command: Command,
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    match command {
        Command::Export {
            format,
            session,
            output,
            turn_range,
            include_notes,
            params,
        } => {
            let resolved =
                resolve_export_args(format, session, output, turn_range, include_notes, params)?;
            export_session(
                providers,
                resolved.format,
                &resolved.session,
                resolved.output.as_deref(),
                resolved.turn_range.as_deref(),
                resolved.include_notes,
            )
        }
        Command::Index {
            provider,
            force,
            accept_download,
            params,
        } => {
            let (provider, force, accept_download) =
                resolve_index_args(provider, force, accept_download, params)?;
            run_index(providers, provider, force, accept_download)
        }
        Command::Search {
            query,
            query_file,
            stdin,
            limit,
            cursor,
            json,
            watch,
            watch_interval_ms,
            watch_iterations,
            debug_search,
            hybrid_weight,
            params,
        } => dispatch_search_command(
            providers,
            filters,
            SearchDispatchArgs {
                query,
                query_file,
                stdin,
                limit,
                cursor,
                json,
                hybrid_weight,
                params,
                mode: if watch {
                    SearchDispatchMode::Watch {
                        interval_ms: watch_interval_ms,
                        iterations: watch_iterations,
                    }
                } else {
                    SearchDispatchMode::Once { debug_search }
                },
            },
        ),
        Command::Show {
            reference,
            format,
            include_context,
            params,
        } => {
            let (reference, format, include_context) =
                resolve_show_args(reference, format, include_context, params)?;
            show_command(providers, &reference, format, include_context)
        }
        Command::Diff {
            session1,
            session2,
            context,
            json,
        } => diff_command(providers, &session1, &session2, context, json),
        _ => unreachable!("lookup dispatch received unrelated command"),
    }
}
