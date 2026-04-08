use clap::{Args, Parser, Subcommand};

use crate::output::OutputFormat;

#[derive(Parser)]
#[command(
    name = "agent-history",
    about = "Browse and search AI coding agent conversation history"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List sessions across all providers
    List(ListArgs),
    /// Show a full conversation
    Show(ShowArgs),
    /// Search across all conversations
    Search(SearchArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by provider name (e.g. "claude", "codex")
    #[arg(long, short)]
    pub tool: Option<String>,

    /// Filter by project path (substring match)
    #[arg(long, short)]
    pub project: Option<String>,

    /// Only show sessions after this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of sessions to display
    #[arg(long, short, default_value = "20")]
    pub limit: usize,

    /// Output format
    #[arg(long, default_value = "table", value_enum)]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ShowArgs {
    /// Session ID or unique prefix
    pub session_id: String,

    /// Hide tool call and tool result messages
    #[arg(long)]
    pub no_tools: bool,

    /// Hide thinking/reasoning blocks
    #[arg(long)]
    pub no_thinking: bool,

    /// Output format
    #[arg(long, default_value = "plain", value_enum)]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Text to search for (case-insensitive substring match)
    pub query: String,

    /// Filter by provider name
    #[arg(long, short)]
    pub tool: Option<String>,

    /// Filter by project path (substring match)
    #[arg(long, short)]
    pub project: Option<String>,

    /// Only search sessions after this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Number of context lines around each match
    #[arg(long, short, default_value = "2")]
    pub context: usize,
}
