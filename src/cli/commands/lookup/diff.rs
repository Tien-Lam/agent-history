use aghist::schema_fragments::{DIFF_CONTEXT_DEFAULT, DIFF_CONTEXT_MAX};
use clap::Args;

use super::parse_reference_selector;
use crate::cli::parsers::parse_usize_at_most;

#[derive(Args)]
pub(crate) struct DiffCommand {
    /// First session ref (e.g. `claude-code/abc-123` or `laptop:claude-code/abc-123`).
    #[arg(value_name = "SESSION1", value_parser = parse_reference_selector)]
    pub(crate) session1: String,

    /// Second session ref (e.g. `claude-code/def-456` or `laptop:claude-code/def-456`).
    #[arg(value_name = "SESSION2", value_parser = parse_reference_selector)]
    pub(crate) session2: String,

    /// Context lines around each changed hunk (default 2).
    #[arg(long, short = 'c', default_value_t = DIFF_CONTEXT_DEFAULT, value_name = "N", value_parser = parse_diff_context)]
    pub(crate) context: usize,

    /// Force JSON output (default: unified diff text on TTY, JSON on pipe).
    #[arg(long)]
    pub(crate) json: bool,
}

fn parse_diff_context(raw: &str) -> Result<usize, String> {
    parse_usize_at_most(raw, "diff context", DIFF_CONTEXT_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_diff_context_accepts_zero() {
        assert_eq!(parse_diff_context("0").unwrap(), 0);
    }

    #[test]
    fn parse_diff_context_rejects_values_above_max() {
        assert_eq!(
            parse_diff_context(&(DIFF_CONTEXT_MAX + 1).to_string()).unwrap_err(),
            format!("diff context must be at most {DIFF_CONTEXT_MAX}")
        );
    }
}
