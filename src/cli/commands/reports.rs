use clap::Args;

use aghist::schema_fragments::{
    REPORT_DAYS_MAX, REPORT_SECTION_LIMIT_MAX, USAGE_LIMIT_DEFAULT, USAGE_LIMIT_MAX,
};

use super::super::resolvers::parse_usage_group_by;

#[derive(Args)]
pub(crate) struct UsageCommand {
    /// Group rows by `model` (default), `provider`, or `project`.
    #[arg(long, default_value = "model", value_parser = parse_usage_group_by, value_name = "DIM")]
    pub(crate) by: aghist::usage::GroupBy,

    /// Cap rows after sorting. Totals always cover every matching session,
    /// even those clipped from `rows`.
    #[arg(long, short = 'n', default_value_t = USAGE_LIMIT_DEFAULT, value_parser = parse_usage_limit)]
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

    /// Cap the decisions section.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.decisions, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) decisions: usize,

    /// Cap the todos section.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.todos, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) todos: usize,

    /// Cap the threads section.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.threads, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) threads: usize,

    /// Cap the top-files section.
    #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.files, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) files: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct ReportCommand {
    /// Window length in days. Mutually exclusive with `--week`/`--month`.
    #[arg(long, value_name = "N", conflicts_with_all = ["week", "month"], value_parser = parse_report_days)]
    pub(crate) days: Option<i64>,

    /// Shorthand for `--days 7`.
    #[arg(long, conflicts_with_all = ["days", "month"])]
    pub(crate) week: bool,

    /// Shorthand for `--days 30`.
    #[arg(long, conflicts_with_all = ["days", "week"])]
    pub(crate) month: bool,

    /// Cap the top-projects section.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.top_projects, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) top_projects: usize,

    /// Cap the decisions section.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.decisions, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) decisions: usize,

    /// Cap the todos section.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.todos, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) todos: usize,

    /// Cap the threads section.
    #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.threads, value_name = "N", value_parser = parse_section_limit)]
    pub(crate) threads: usize,

    /// Emit the structured JSON envelope instead of Markdown.
    #[arg(long)]
    pub(crate) json: bool,
}

fn parse_usage_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_usize(raw, "usage limit", USAGE_LIMIT_MAX)
}

fn parse_section_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_usize(raw, "section limit", REPORT_SECTION_LIMIT_MAX)
}

fn parse_positive_bounded_usize(raw: &str, label: &str, max: usize) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|e| format!("invalid {label}: {e}"))?;
    if value == 0 {
        Err(format!("{label} must be at least 1"))
    } else if value > max {
        Err(format!("{label} must be at most {max}"))
    } else {
        Ok(value)
    }
}

fn parse_report_days(raw: &str) -> Result<i64, String> {
    let value = raw
        .parse::<i64>()
        .map_err(|e| format!("invalid report days: {e}"))?;
    if value < 1 {
        Err("report days must be at least 1".to_string())
    } else if value > REPORT_DAYS_MAX {
        Err(format!("report days must be at most {REPORT_DAYS_MAX}"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_report_days, parse_section_limit, parse_usage_limit};
    use aghist::schema_fragments::{REPORT_DAYS_MAX, REPORT_SECTION_LIMIT_MAX, USAGE_LIMIT_MAX};

    #[test]
    fn bounded_report_parsers_reject_zero() {
        assert!(parse_usage_limit("0")
            .unwrap_err()
            .contains("must be at least 1"));
        assert!(parse_section_limit("0")
            .unwrap_err()
            .contains("must be at least 1"));
        assert_eq!(
            parse_report_days("0").unwrap_err(),
            "report days must be at least 1"
        );
    }

    #[test]
    fn bounded_report_parsers_reject_values_above_max() {
        assert!(parse_usage_limit(&(USAGE_LIMIT_MAX + 1).to_string())
            .unwrap_err()
            .contains("must be at most"));
        assert!(
            parse_section_limit(&(REPORT_SECTION_LIMIT_MAX + 1).to_string())
                .unwrap_err()
                .contains("must be at most")
        );
        assert_eq!(
            parse_report_days(&(REPORT_DAYS_MAX + 1).to_string()).unwrap_err(),
            format!("report days must be at most {REPORT_DAYS_MAX}")
        );
    }
}
