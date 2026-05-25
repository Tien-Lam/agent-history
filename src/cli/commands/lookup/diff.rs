use clap::Args;

use super::parse_reference_selector;

#[derive(Args)]
pub(crate) struct DiffCommand {
    /// First session ref (e.g. `claude-code/abc-123` or `laptop:claude-code/abc-123`).
    #[arg(value_name = "SESSION1", value_parser = parse_reference_selector)]
    pub(crate) session1: String,

    /// Second session ref (e.g. `claude-code/def-456` or `laptop:claude-code/def-456`).
    #[arg(value_name = "SESSION2", value_parser = parse_reference_selector)]
    pub(crate) session2: String,

    /// Context lines around each changed hunk (default 2).
    #[arg(long, short = 'c', default_value_t = 2, value_name = "N")]
    pub(crate) context: usize,

    /// Force JSON output (default: unified diff text on TTY, JSON on pipe).
    #[arg(long)]
    pub(crate) json: bool,
}
