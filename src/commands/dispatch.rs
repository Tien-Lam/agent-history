use super::super::cli::{Cli, Command, FilterArgs};
use super::analysis_dispatch::dispatch_analysis_command;
use super::filtering::resolve_metadata_filter;
use super::install::{self_update, uninstall};
use super::list::list_sessions;
use super::lookup_dispatch::dispatch_lookup_command;
use super::metadata_dispatch::dispatch_metadata_command;
use super::reports_dispatch::dispatch_report_command;
use super::system::{run_mcp, schema_command};
use super::tui::run_tui;
use aghist::cli_error::{ErrorEnvelope, EXIT_USAGE};
use aghist::output::{CommandKind, OutputMode};
use aghist::search;
use aghist::{config, provider};

#[derive(Clone, Copy)]
struct OutputFlags {
    json: bool,
    ndjson: bool,
}

impl OutputFlags {
    fn mode(self, kind: CommandKind) -> OutputMode {
        OutputMode::resolve(self.json, self.ndjson, kind)
    }
}

struct DispatchContext<'a> {
    providers: &'a [Box<dyn provider::HistoryProvider>],
    filters: &'a FilterArgs,
    output: OutputFlags,
}

pub(crate) fn run(cli: Cli) -> Result<i32, ErrorEnvelope> {
    if let Some(exit) = reject_conflicting_output_flags(cli.json, cli.ndjson) {
        return Ok(exit);
    }

    let config = load_config()?;
    clear_search_index_if_requested(cli.reindex)?;
    let providers = detect_enabled_providers(&config);
    let Cli {
        list,
        limit,
        cursor,
        reindex: _,
        json,
        ndjson,
        filters,
        command,
    } = cli;

    match command {
        Some(Command::Mcp) => run_mcp_with_config(providers, &config),
        command => {
            let ctx = DispatchContext {
                providers: &providers,
                filters: &filters,
                output: OutputFlags { json, ndjson },
            };
            if let Some(exit) = dispatch_command(command, &ctx)? {
                return Ok(exit);
            }
            if list {
                return dispatch_list(limit, cursor.as_deref(), &ctx);
            }

            run_tui(providers, config)
        }
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

fn load_config() -> Result<config::Config, ErrorEnvelope> {
    config::Config::try_load().map_err(|e| {
        ErrorEnvelope::new("config-error", format!("{e}"))
            .with_hint("Fix the TOML or set AGHIST_CONFIG to a known-good config file.")
    })
}

fn detect_enabled_providers(config: &config::Config) -> Vec<Box<dyn provider::HistoryProvider>> {
    let enabled = config.enabled_providers();
    provider::detect_all_providers()
        .into_iter()
        .filter(|p| enabled.contains(&p.provider()))
        .collect()
}

fn run_mcp_with_config(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
    config: &config::Config,
) -> Result<i32, ErrorEnvelope> {
    // MCP gets a narrower view than the rest of the CLI: users can hide
    // providers from MCP clients without disabling them locally.
    let exposed = config.mcp_exposed_providers();
    let providers = providers
        .into_iter()
        .filter(|p| exposed.contains(&p.provider()))
        .collect();
    run_mcp(providers)
}

fn dispatch_command(
    command: Option<Command>,
    ctx: &DispatchContext<'_>,
) -> Result<Option<i32>, ErrorEnvelope> {
    let Some(command) = command else {
        return Ok(None);
    };
    let exit = match command {
        Command::Mcp => unreachable!("MCP is handled before borrowed dispatch"),
        Command::Schema {
            subcommand,
            list,
            all,
        } => schema_command(subcommand.as_deref(), list, all)?,
        Command::Update => self_update()?,
        Command::Uninstall => uninstall()?,
        cmd @ (Command::Export(_)
        | Command::Index(_)
        | Command::Search(_)
        | Command::Show(_)
        | Command::Diff(_)) => dispatch_lookup_command(cmd, ctx.providers, ctx.filters)?,
        cmd @ (Command::Track(_)
        | Command::Decisions(_)
        | Command::Todos(_)
        | Command::Threads(_)) => dispatch_analysis_command(cmd, ctx.providers, ctx.filters)?,
        cmd @ (Command::Sources { .. }
        | Command::Health
        | Command::Note { .. }
        | Command::Tag { .. }
        | Command::Star { .. }
        | Command::Unstar { .. }
        | Command::Stars { .. }) => {
            dispatch_metadata_command(cmd, ctx.providers, ctx.output.mode(CommandKind::OneShot))?
        }
        cmd @ (Command::Usage(_) | Command::Project(_) | Command::Report(_)) => {
            dispatch_report_command(cmd, ctx.providers, ctx.filters)?
        }
    };
    Ok(Some(exit))
}

fn dispatch_list(
    limit: usize,
    cursor: Option<&str>,
    ctx: &DispatchContext<'_>,
) -> Result<i32, ErrorEnvelope> {
    let metadata_keys = resolve_metadata_filter(ctx.filters)?;
    list_sessions(
        ctx.providers,
        ctx.output.mode(CommandKind::Streaming),
        limit,
        cursor,
        ctx.filters,
        metadata_keys.as_ref(),
    )
}
