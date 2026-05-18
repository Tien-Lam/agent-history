use super::super::cli::{Cli, CommandTarget, ContextCommand, ContextFreeCommand};
use super::analysis_dispatch::dispatch_analysis_command;
use super::context::CommandContext;
use super::install::{self_update, uninstall};
use super::list::list_sessions;
use super::lookup_dispatch::dispatch_lookup_command;
use super::metadata_dispatch::dispatch_metadata_command;
use super::reports_dispatch::dispatch_report_command;
use super::system::{run_mcp_server, schema_command};
use super::tui::run_tui;
use aghist::cli_error::{ErrorEnvelope, EXIT_USAGE};
use aghist::output::CommandKind;
use aghist::search;

pub(crate) fn run(cli: Cli) -> Result<i32, ErrorEnvelope> {
    if let Some(exit) = reject_conflicting_output_flags(cli.json, cli.ndjson) {
        return Ok(exit);
    }

    let Cli {
        list,
        limit,
        cursor,
        reindex,
        json,
        ndjson,
        filters,
        command,
    } = cli;

    let command = match command.map(CommandTarget::from) {
        Some(CommandTarget::ContextFree(command)) => return dispatch_context_free_command(command),
        other => other,
    };

    let ctx = CommandContext::load(filters, json, ndjson)?;
    clear_search_index_if_requested(reindex)?;

    match command {
        Some(CommandTarget::Mcp) => {
            let server = ctx.into_mcp_server();
            run_mcp_server(&server)
        }
        Some(CommandTarget::Context(command)) => dispatch_command(command, &ctx),
        None => {
            if list {
                return dispatch_list(limit, cursor.as_deref(), &ctx);
            }

            let (providers, config) = ctx.into_tui_parts();
            run_tui(providers, config)
        }
    }
}

fn dispatch_context_free_command(command: ContextFreeCommand) -> Result<i32, ErrorEnvelope> {
    match command {
        ContextFreeCommand::Schema {
            subcommand,
            list,
            all,
        } => schema_command(subcommand.as_deref(), list, all),
        ContextFreeCommand::Update => self_update(),
        ContextFreeCommand::Uninstall => uninstall(),
    }
}

fn clear_search_index_if_requested(reindex: bool) -> Result<(), ErrorEnvelope> {
    if !reindex {
        return Ok(());
    }
    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open search index {}: {e}", index_dir.display()),
        )
        .with_hint("Set AGHIST_INDEX_DIR to a writable directory, or fix index permissions.")
    })?;
    index.clear().map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to clear search index {}: {e}", index_dir.display()),
        )
        .with_hint("Set AGHIST_INDEX_DIR to a writable directory, or fix index permissions.")
    })?;
    eprintln!("Search index cleared. Will rebuild on next launch.");
    Ok(())
}

fn reject_conflicting_output_flags(json: bool, ndjson: bool) -> Option<i32> {
    if json && ndjson {
        ErrorEnvelope::new("usage", "--json and --ndjson are mutually exclusive")
            .with_hint("Pick one. Without either, output auto-detects: JSON/NDJSON on a pipe, human format on a TTY.")
            .emit();
        return Some(EXIT_USAGE);
    }
    None
}

fn dispatch_command(command: ContextCommand, ctx: &CommandContext) -> Result<i32, ErrorEnvelope> {
    match command {
        ContextCommand::Lookup(command) => dispatch_lookup_command(command, ctx),
        ContextCommand::Analysis(command) => dispatch_analysis_command(command, ctx),
        ContextCommand::Metadata(command) => dispatch_metadata_command(command, ctx),
        ContextCommand::Reports(command) => dispatch_report_command(command, ctx),
    }
}

fn dispatch_list(
    limit: usize,
    cursor: Option<&str>,
    ctx: &CommandContext,
) -> Result<i32, ErrorEnvelope> {
    let metadata_keys = ctx.metadata_filter_keys()?;
    list_sessions(
        ctx.providers(),
        ctx.scope(),
        ctx.output_mode(CommandKind::Streaming),
        limit,
        cursor,
        ctx.filters(),
        metadata_keys.as_ref(),
    )
}
