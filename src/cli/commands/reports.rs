use clap::Args;

use super::super::resolvers::parse_usage_group_by;

#[derive(Args)]
pub(crate) struct UsageCommand {
    /// Group rows by `model` (default), `provider`, or `project`.
    #[arg(long, default_value = "model", value_parser = parse_usage_group_by, value_name = "DIM")]
    pub(crate) by: aghist::usage::GroupBy,

    /// Cap rows after sorting (0 = no limit). Totals always cover every
    /// matching session, even those clipped from `rows`.
    #[arg(long, short = 'n', default_value_t = 0)]
    pub(crate) limit: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct ProjectCommand {
    /// Project name. Matched as a case-insensitive substring against
    /// each session's `project_name`.
    #[arg(value_name = "NAME")]
    pub(crate) name: String,

    /// Cap the decisions section. 0 = no cap.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.decisions, value_name = "N")]
    pub(crate) decisions: usize,

    /// Cap the todos section. 0 = no cap.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.todos, value_name = "N")]
    pub(crate) todos: usize,

    /// Cap the threads section. 0 = no cap.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.threads, value_name = "N")]
    pub(crate) threads: usize,

    /// Cap the top-files section. 0 = no cap.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.files, value_name = "N")]
    pub(crate) files: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct ReportCommand {
    /// Window length in days. Mutually exclusive with `--week`/`--month`.
    #[arg(long, value_name = "N", conflicts_with_all = ["week", "month"])]
    pub(crate) days: Option<i64>,

    /// Shorthand for `--days 7`.
    #[arg(long, conflicts_with_all = ["days", "month"])]
    pub(crate) week: bool,

    /// Shorthand for `--days 30`.
    #[arg(long, conflicts_with_all = ["days", "week"])]
    pub(crate) month: bool,

    /// Cap the top-projects section. 0 = no cap.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.top_projects, value_name = "N")]
    pub(crate) top_projects: usize,

    /// Cap the decisions section. 0 = no cap.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.decisions, value_name = "N")]
    pub(crate) decisions: usize,

    /// Cap the todos section. 0 = no cap.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.todos, value_name = "N")]
    pub(crate) todos: usize,

    /// Cap the threads section. 0 = no cap.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.threads, value_name = "N")]
    pub(crate) threads: usize,

    /// Emit the structured JSON envelope instead of Markdown.
    #[arg(long)]
    pub(crate) json: bool,
}
