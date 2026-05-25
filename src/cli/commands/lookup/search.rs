use std::path::PathBuf;

use aghist::schema_fragments::{
    SEARCH_HYBRID_WEIGHT_DEFAULT, SEARCH_LIMIT_DEFAULT, SEARCH_LIMIT_MAX,
    SEARCH_WATCH_INTERVAL_MS_DEFAULT, SEARCH_WATCH_INTERVAL_MS_MAX,
    SEARCH_WATCH_ITERATIONS_DEFAULT, SEARCH_WATCH_ITERATIONS_MAX,
};
use clap::Args;

use crate::cli::{parse_cli_path, parse_cursor_token};

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)] // CLI flag struct: clap requires bool fields per flag
pub(crate) struct SearchCommand {
    /// Tantivy query string (matches content + project fields).
    ///
    /// Omit when reading the query from `--query-file`, `--stdin`, or `--params`.
    #[arg(conflicts_with_all = ["query_file", "stdin", "params"])]
    pub(crate) query: Option<String>,

    /// Read the query from a file (use `-` for stdin).
    ///
    /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
    /// without escaping. Trailing whitespace is stripped.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["stdin", "params"], value_parser = parse_cli_path)]
    pub(crate) query_file: Option<PathBuf>,

    /// Read the query from standard input (read until EOF).
    ///
    /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
    /// without escaping. Trailing whitespace is stripped.
    #[arg(long, conflicts_with = "params")]
    pub(crate) stdin: bool,

    /// Maximum number of hits to return
    #[arg(
        long,
        short = 'n',
        default_value_t = SEARCH_LIMIT_DEFAULT,
        value_parser = parse_search_limit,
        conflicts_with = "params"
    )]
    pub(crate) limit: usize,

    /// Opaque pagination cursor from a prior `meta.next_cursor`.
    #[arg(long, conflicts_with = "params", value_parser = parse_cursor_token)]
    pub(crate) cursor: Option<String>,

    /// Force JSON output (default: JSON on pipe, table on TTY)
    #[arg(long, conflicts_with = "params")]
    pub(crate) json: bool,

    /// Long-running stream: emit one NDJSON line per new hit as sessions land.
    ///
    /// First poll backfills all existing matches up to `--limit`, then each
    /// subsequent poll emits only previously-unseen `(session_id, message_id)`
    /// hits. Useful for an "agent of agents" watching another agent's progress.
    /// Output is NDJSON regardless of TTY; `--json` is implied.
    #[arg(long, conflicts_with = "params")]
    pub(crate) watch: bool,

    /// Poll interval in milliseconds when `--watch` is set (default 2000).
    #[arg(
        long,
        default_value_t = SEARCH_WATCH_INTERVAL_MS_DEFAULT,
        value_name = "MS",
        value_parser = parse_watch_interval_ms,
        conflicts_with = "params"
    )]
    pub(crate) watch_interval_ms: u64,

    /// Stop watch mode after N polls (0 = run until interrupted; default 0).
    ///
    /// Mostly useful for tests and one-shot snapshots.
    #[arg(long, default_value_t = SEARCH_WATCH_ITERATIONS_DEFAULT, value_name = "N", value_parser = parse_watch_iterations, conflicts_with = "params")]
    pub(crate) watch_iterations: u32,

    /// Show BM25 score breakdown per result (Tantivy explanation tree).
    /// Useful for tuning relevance and surfacing ranking surprises.
    #[arg(long, conflicts_with = "params")]
    pub(crate) debug_search: bool,

    /// Reciprocal Rank Fusion weight on the semantic side, in `[0.0, 1.0]`.
    ///
    /// `0.0` (default) -> lexical-only BM25, identical to omitting the flag.
    /// `0.5` -> equal RRF blend of BM25 and `FastEmbed` cosine ranks.
    /// `1.0` -> semantic-only.
    ///
    /// Fails open: if the binary lacks the `embeddings` feature, or no
    /// consent / embedding store exists yet, the search degrades to
    /// lexical-only regardless of this value (see `meta.engine` in JSON
    /// output). When hybrid is active, `score` becomes the RRF fused
    /// score (small, ~0-0.03) - not a BM25 score.
    #[arg(
        long,
        default_value_t = SEARCH_HYBRID_WEIGHT_DEFAULT,
        value_name = "FLOAT",
        value_parser = parse_hybrid_weight,
        conflicts_with = "params"
    )]
    pub(crate) hybrid_weight: f32,

    /// JSON request body containing all one-shot params at once. Mutually
    /// exclusive with other flags. Schema:
    /// `{query, limit?, cursor?, json?, debug_search?, hybrid_weight?}`.
    /// The `query` field carries the literal query string; use `--query-file`
    /// / `--stdin` for file/stdin input.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

fn parse_search_limit(raw: &str) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|e| format!("invalid search limit: {e}"))?;
    match value {
        0 => Err("search limit must be at least 1".to_string()),
        value if value > SEARCH_LIMIT_MAX => {
            Err(format!("search limit must be at most {SEARCH_LIMIT_MAX}"))
        }
        value => Ok(value),
    }
}

fn parse_watch_interval_ms(raw: &str) -> Result<u64, String> {
    let value = raw
        .parse::<u64>()
        .map_err(|e| format!("invalid watch interval: {e}"))?;
    match value {
        0 => Err("watch interval must be at least 1 millisecond".to_string()),
        value if value > SEARCH_WATCH_INTERVAL_MS_MAX => Err(format!(
            "watch interval must be at most {SEARCH_WATCH_INTERVAL_MS_MAX} milliseconds"
        )),
        value => Ok(value),
    }
}

fn parse_watch_iterations(raw: &str) -> Result<u32, String> {
    let value = raw
        .parse::<u32>()
        .map_err(|e| format!("invalid watch iterations: {e}"))?;
    if value > SEARCH_WATCH_ITERATIONS_MAX {
        Err(format!(
            "watch iterations must be at most {SEARCH_WATCH_ITERATIONS_MAX}"
        ))
    } else {
        Ok(value)
    }
}

fn parse_hybrid_weight(raw: &str) -> Result<f32, String> {
    let value = raw
        .parse::<f32>()
        .map_err(|e| format!("invalid hybrid weight: {e}"))?;
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err("hybrid weight must be a finite number between 0.0 and 1.0".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_limit_rejects_zero() {
        assert_eq!(
            parse_search_limit("0").unwrap_err(),
            "search limit must be at least 1"
        );
    }

    #[test]
    fn parse_search_limit_rejects_values_above_max() {
        assert_eq!(
            parse_search_limit(&(SEARCH_LIMIT_MAX + 1).to_string()).unwrap_err(),
            format!("search limit must be at most {SEARCH_LIMIT_MAX}")
        );
    }

    #[test]
    fn parse_watch_interval_ms_rejects_out_of_range_values() {
        assert_eq!(
            parse_watch_interval_ms("0").unwrap_err(),
            "watch interval must be at least 1 millisecond"
        );
        assert_eq!(
            parse_watch_interval_ms(&(SEARCH_WATCH_INTERVAL_MS_MAX + 1).to_string()).unwrap_err(),
            format!("watch interval must be at most {SEARCH_WATCH_INTERVAL_MS_MAX} milliseconds")
        );
    }

    #[test]
    fn parse_watch_iterations_allows_zero_and_rejects_values_above_max() {
        assert_eq!(parse_watch_iterations("0").unwrap(), 0);
        assert_eq!(
            parse_watch_iterations(&(SEARCH_WATCH_ITERATIONS_MAX + 1).to_string()).unwrap_err(),
            format!("watch iterations must be at most {SEARCH_WATCH_ITERATIONS_MAX}")
        );
    }
}
