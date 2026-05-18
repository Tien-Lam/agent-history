use aghist::cli_error::ErrorEnvelope;

use super::super::cli::ReportsCommand;
use super::context::CommandContext;
use super::reports::{project_command, report_command, usage_command};

pub(crate) fn dispatch_report_command(
    command: ReportsCommand,
    ctx: &CommandContext,
) -> Result<i32, ErrorEnvelope> {
    let filters = ctx.filters();
    let providers = ctx.providers();
    let scope = ctx.scope();
    let metadata_keys = ctx.metadata_filter_keys()?;
    match command {
        ReportsCommand::Usage(args) => usage_command(
            providers,
            scope,
            filters,
            metadata_keys.as_ref(),
            args.by,
            args.limit,
            args.json,
        ),
        ReportsCommand::Project(args) => {
            let limits = aghist::project::ProjectLimits {
                decisions: args.decisions,
                todos: args.todos,
                threads: args.threads,
                files: args.files,
            };
            project_command(
                providers,
                scope,
                filters,
                metadata_keys.as_ref(),
                &args.name,
                limits,
                args.json,
            )
        }
        ReportsCommand::Report(args) => {
            let window_days = if args.month {
                30
            } else if args.week {
                7
            } else {
                args.days.unwrap_or(7)
            };
            let limits = aghist::report::ReportLimits {
                top_projects: args.top_projects,
                decisions: args.decisions,
                todos: args.todos,
                threads: args.threads,
            };
            report_command(
                providers,
                scope,
                filters,
                metadata_keys.as_ref(),
                window_days,
                limits,
                args.json,
            )
        }
    }
}
