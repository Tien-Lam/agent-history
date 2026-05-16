use aghist::cli_error::ErrorEnvelope;
use aghist::provider;

use super::super::cli::{Command, FilterArgs};
use super::filtering::resolve_metadata_filter;
use super::reports::{project_command, report_command, usage_command};

pub(crate) fn dispatch_report_command(
    command: Command,
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    let metadata_keys = resolve_metadata_filter(filters)?;
    match command {
        Command::Usage(args) => usage_command(
            providers,
            filters,
            metadata_keys.as_ref(),
            args.by,
            args.limit,
            args.json,
        ),
        Command::Project(args) => {
            let limits = aghist::project::ProjectLimits {
                decisions: args.decisions,
                todos: args.todos,
                threads: args.threads,
                files: args.files,
            };
            project_command(
                providers,
                filters,
                metadata_keys.as_ref(),
                &args.name,
                limits,
                args.json,
            )
        }
        Command::Report(args) => {
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
                filters,
                metadata_keys.as_ref(),
                window_days,
                limits,
                args.json,
            )
        }
        _ => unreachable!("report dispatch received unrelated command"),
    }
}
