mod commands;
mod filters;
mod resolvers;

use std::path::{Path, PathBuf};

use clap::Parser;

use aghist::schema_fragments::{
    CLI_PATH_MAX_BYTES, CURSOR_TOKEN_MAX_BYTES, LIST_LIMIT_DEFAULT, LIST_LIMIT_MAX,
};

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
    #[arg(long, requires = "list", value_parser = parse_cursor_token)]
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

pub(crate) fn parse_cursor_token(raw: &str) -> Result<String, String> {
    if raw.len() > CURSOR_TOKEN_MAX_BYTES {
        return Err(format!(
            "cursor token must be at most {CURSOR_TOKEN_MAX_BYTES} bytes"
        ));
    }
    Ok(raw.to_string())
}

pub(crate) fn parse_cli_path(raw: &str) -> Result<PathBuf, String> {
    validate_cli_path(raw)?;
    Ok(PathBuf::from(raw))
}

pub(crate) fn validate_cli_path(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    let display = path.as_os_str().to_string_lossy();
    if display.len() > CLI_PATH_MAX_BYTES {
        Err(format!(
            "file path must be at most {CLI_PATH_MAX_BYTES} bytes"
        ))
    } else {
        Ok(())
    }
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

    #[test]
    fn parse_cursor_token_rejects_oversized_values() {
        let raw = "c".repeat(CURSOR_TOKEN_MAX_BYTES + 1);
        let err = parse_cursor_token(&raw).unwrap_err();
        assert!(err.contains(&CURSOR_TOKEN_MAX_BYTES.to_string()));
    }

    #[test]
    fn parse_cli_path_rejects_oversized_values() {
        let raw = "p".repeat(CLI_PATH_MAX_BYTES + 1);
        let err = parse_cli_path(&raw).unwrap_err();
        assert!(err.contains(&CLI_PATH_MAX_BYTES.to_string()));
    }
}
