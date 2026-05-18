mod commands;
mod filters;
mod resolvers;

use clap::Parser;

use aghist::schema_fragments::LIST_LIMIT_DEFAULT;

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
    #[arg(long, default_value_t = LIST_LIMIT_DEFAULT, requires = "list")]
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
