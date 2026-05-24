mod commands;
mod filters;
mod resolvers;

use clap::Parser;

use aghist::schema_fragments::{LIST_LIMIT_DEFAULT, LIST_LIMIT_MAX};

pub(crate) use commands::{
    AnalysisCommand, Command, CommandTarget, ContextCommand, ContextFreeCommand, LookupCommand,
    MetadataCommand, NoteCommand, ReportsCommand, SourcesCommand, TagCommand,
};
pub(crate) use filters::FilterArgs;
pub(crate) use resolvers::{
    resolve_export_args, resolve_index_args, resolve_search_args, resolve_show_args, SearchArgs,
    ShowFormat,
};

#[derive(Parser)]
#[command(
    name = "aghist",
    version,
    about = "Browse and search AI agent conversation history"
)]
#[allow(clippy::struct_excessive_bools)] // CLI flag struct: clap requires bool fields per flag
pub(crate) struct Cli {
    /// List sessions without opening the TUI
    #[arg(long)]
    pub(crate) list: bool,

    /// Maximum number of sessions to return when paired with `--list`.
    /// JSON output includes `meta.next_cursor` if more results remain.
    #[arg(long, default_value_t = LIST_LIMIT_DEFAULT, requires = "list", value_parser = parse_list_limit)]
    pub(crate) limit: usize,

    /// Opaque pagination cursor (from a prior `meta.next_cursor`) for `--list`.
    #[arg(long, requires = "list")]
    pub(crate) cursor: Option<String>,

    /// Force rebuild the search index
    #[arg(long)]
    pub(crate) reindex: bool,

    /// Force JSON output (for one-shot commands like --list, export).
    /// Mutually exclusive with --ndjson.
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// Force newline-delimited JSON output (for streaming commands).
    /// Mutually exclusive with --json.
    #[arg(long, global = true)]
    pub(crate) ndjson: bool,

    #[command(flatten)]
    pub(crate) filters: FilterArgs,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

fn parse_list_limit(raw: &str) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|e| format!("invalid list limit: {e}"))?;
    match value {
        0 => Err("list limit must be at least 1".to_string()),
        value if value > LIST_LIMIT_MAX => {
            Err(format!("list limit must be at most {LIST_LIMIT_MAX}"))
        }
        value => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_limit_rejects_zero() {
        assert_eq!(
            parse_list_limit("0").unwrap_err(),
            "list limit must be at least 1"
        );
    }

    #[test]
    fn parse_list_limit_rejects_values_above_max() {
        assert_eq!(
            parse_list_limit(&(LIST_LIMIT_MAX + 1).to_string()).unwrap_err(),
            format!("list limit must be at most {LIST_LIMIT_MAX}")
        );
    }
}
