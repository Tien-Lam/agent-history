use aghist::cli_error::ErrorEnvelope;
use aghist::provider;

use super::super::cli::{Command, FilterArgs};
use super::reports::{project_command, report_command, usage_command};

pub(crate) fn dispatch_report_command(
    command: Command,
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    match command {
        Command::Usage { by, limit, json } => usage_command(providers, filters, by, limit, json),
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
            project_command(providers, filters, &name, limits, json)
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
            report_command(providers, filters, window_days, limits, json)
        }
        _ => unreachable!("report dispatch received unrelated command"),
    }
}
