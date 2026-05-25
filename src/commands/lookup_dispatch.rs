use aghist::cli_error::ErrorEnvelope;

use super::super::cli::{
    resolve_export_args, resolve_index_args, resolve_show_args, LookupCommand,
};
use super::context::CommandContext;
use super::diff::diff_command;
use super::export::export_session;
use super::index::run_index;
use super::search_dispatch::{dispatch_search_command, SearchDispatchArgs, SearchDispatchMode};
use super::show::show_command;

pub(crate) fn dispatch_lookup_command(
    command: LookupCommand,
    ctx: &CommandContext,
) -> Result<i32, ErrorEnvelope> {
    match command {
        LookupCommand::Export(args) => {
            let resolved = resolve_export_args(
                args.format,
                args.session,
                args.output,
                args.turn_range,
                args.include_notes,
                args.params,
            )?;
            export_session(
                ctx.providers(),
                ctx.scope(),
                resolved.format,
                &resolved.session,
                resolved.output.as_deref(),
                resolved.turn_range.as_deref(),
                resolved.include_notes,
            )
        }
        LookupCommand::Index(args) => {
            let (provider, force, accept_download) =
                resolve_index_args(args.provider, args.force, args.accept_download, args.params)?;
            run_index(
                ctx.providers(),
                ctx.scope(),
                provider,
                force,
                accept_download,
            )
        }
        LookupCommand::Search(args) => dispatch_search_command(
            ctx,
            SearchDispatchArgs {
                query: args.query,
                query_file: args.query_file,
                stdin: args.stdin,
                limit: args.limit,
                cursor: args.cursor,
                json: args.json,
                hybrid_weight: args.hybrid_weight,
                params: args.params,
                mode: if args.watch {
                    SearchDispatchMode::Watch {
                        interval_ms: args.watch_interval_ms,
                        iterations: args.watch_iterations,
                    }
                } else {
                    SearchDispatchMode::Once {
                        debug_search: args.debug_search,
                    }
                },
            },
        ),
        LookupCommand::Show(args) => {
            let (reference, format, include_context) = resolve_show_args(
                args.reference,
                args.format,
                args.include_context,
                args.params,
            )?;
            show_command(
                ctx.providers(),
                ctx.scope(),
                &reference,
                format,
                include_context,
            )
        }
        LookupCommand::Diff(args) => diff_command(
            ctx.providers(),
            ctx.scope(),
            &args.session1,
            &args.session2,
            args.context,
            ctx.json_only_output(args.json, "diff")?,
        ),
    }
}
