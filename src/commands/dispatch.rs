use super::super::cli::{
    resolve_export_args, resolve_index_args, resolve_show_args, Cli, Command, FilterArgs,
    SourcesCommand,
};
use super::analysis_dispatch::dispatch_analysis_command;
use super::diff::diff_command;
use super::export::export_session;
use super::filtering::resolve_metadata_filter;
use super::health::health_command;
use super::index::run_index;
use super::install::{self_update, uninstall};
use super::list::list_sessions;
use super::metadata::{note_dispatch, star_command, stars_list, tag_dispatch, unstar_command};
use super::reports::{project_command, report_command, usage_command};
use super::search_dispatch::{dispatch_search_command, SearchDispatchArgs, SearchDispatchMode};
use super::show::show_command;
use super::sources::{
    sources_add_remote, sources_command, sources_list_remote, sources_pull_remote,
    sources_remove_remote,
};
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
    clear_search_index_if_requested(cli.reindex);

    if let Some(exit) = reject_conflicting_output_flags(cli.json, cli.ndjson) {
        return Ok(exit);
    }

    let config = load_config()?;
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

fn clear_search_index_if_requested(reindex: bool) {
    if !reindex {
        return;
    }
    let index_dir = search::SearchIndex::default_index_dir();
    if let Ok(index) = search::SearchIndex::open_or_create(&index_dir) {
        let _ = index.clear();
        eprintln!("Search index cleared. Will rebuild on next launch.");
    }
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
        cmd @ (Command::Export { .. }
        | Command::Index { .. }
        | Command::Search { .. }
        | Command::Show { .. }
        | Command::Diff { .. }) => dispatch_lookup_command(cmd, ctx)?,
        cmd @ (Command::Track { .. }
        | Command::Decisions { .. }
        | Command::Todos { .. }
        | Command::Threads { .. }) => dispatch_analysis_command(cmd, ctx.providers, ctx.filters)?,
        cmd @ (Command::Sources { .. }
        | Command::Health
        | Command::Note { .. }
        | Command::Tag { .. }
        | Command::Star { .. }
        | Command::Unstar { .. }
        | Command::Stars { .. }) => dispatch_metadata_command(cmd, ctx)?,
        cmd @ (Command::Usage { .. } | Command::Project { .. } | Command::Report { .. }) => {
            dispatch_report_command(cmd, ctx)?
        }
    };
    Ok(Some(exit))
}

fn dispatch_lookup_command(
    command: Command,
    ctx: &DispatchContext<'_>,
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
                ctx.providers,
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
            run_index(ctx.providers, provider, force, accept_download)
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
            ctx.providers,
            ctx.filters,
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
            show_command(ctx.providers, &reference, format, include_context)
        }
        Command::Diff {
            session1,
            session2,
            context,
            json,
        } => diff_command(ctx.providers, &session1, &session2, context, json),
        _ => unreachable!("lookup dispatch received unrelated command"),
    }
}

fn dispatch_metadata_command(
    command: Command,
    ctx: &DispatchContext<'_>,
) -> Result<i32, ErrorEnvelope> {
    let one_shot = || ctx.output.mode(CommandKind::OneShot);
    match command {
        Command::Sources { command } => {
            dispatch_sources_command(command, ctx.providers, one_shot())
        }
        Command::Health => health_command(ctx.providers, one_shot()),
        Command::Note { command } => note_dispatch(command, one_shot()),
        Command::Tag { command } => tag_dispatch(command, one_shot()),
        Command::Star { reference } => star_command(&reference, one_shot()),
        Command::Unstar { reference } => unstar_command(&reference, one_shot()),
        Command::Stars { reference, json } => {
            let mode = if json { OutputMode::Json } else { one_shot() };
            stars_list(reference.as_deref(), mode)
        }
        _ => unreachable!("metadata dispatch received unrelated command"),
    }
}

fn dispatch_sources_command(
    command: Option<SourcesCommand>,
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    match command {
        None => sources_command(providers, mode),
        Some(SourcesCommand::List) => sources_list_remote(mode),
        Some(SourcesCommand::Add {
            name,
            host,
            path,
            transport,
        }) => sources_add_remote(&name, &host, &path, transport, mode),
        Some(SourcesCommand::Remove { name }) => sources_remove_remote(&name, mode),
        Some(SourcesCommand::Pull { name, all, dry_run }) => {
            sources_pull_remote(name.as_deref(), all, dry_run, mode)
        }
    }
}

fn dispatch_report_command(
    command: Command,
    ctx: &DispatchContext<'_>,
) -> Result<i32, ErrorEnvelope> {
    match command {
        Command::Usage { by, limit, json } => {
            usage_command(ctx.providers, ctx.filters, by, limit, json)
        }
        Command::Project {
            name,
            decisions,
            todos,
            threads,
            files,
            json,
        } => {
            let limits = aghist::project::ProjectLimits {
                decisions,
                todos,
                threads,
                files,
            };
            project_command(ctx.providers, ctx.filters, &name, limits, json)
        }
        Command::Report {
            days,
            week,
            month,
            top_projects,
            decisions,
            todos,
            threads,
            json,
        } => {
            let window_days = if month {
                30
            } else if week {
                7
            } else {
                days.unwrap_or(7)
            };
            let limits = aghist::report::ReportLimits {
                top_projects,
                decisions,
                todos,
                threads,
            };
            report_command(ctx.providers, ctx.filters, window_days, limits, json)
        }
        _ => unreachable!("report dispatch received unrelated command"),
    }
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
