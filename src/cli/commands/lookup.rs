use std::path::PathBuf;

use aghist::export;
use aghist::model::Provider;
use clap::Args;

use super::super::resolvers::{parse_provider_slug, ShowFormat};

#[derive(Args)]
pub(crate) struct ExportCommand {
    /// Output format: md, json, html
    #[arg(
        long,
        short,
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) format: Option<export::ExportFormat>,

    /// Session ID/prefix, `<provider>/<id>`, or `<source>:<provider>/<id>` to export
    #[arg(
        long,
        short,
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) session: Option<String>,

    /// Output file path (defaults to stdout)
    #[arg(long, short, conflicts_with = "params")]
    pub(crate) output: Option<PathBuf>,

    /// Slice the session by 1-based turn range (e.g. `12:25`, `:10`, `5:`, or `7`).
    /// Bounds are inclusive. Out-of-range bounds clamp to the available messages.
    #[arg(long, conflicts_with = "params")]
    pub(crate) turn_range: Option<String>,

    /// Inline private annotations (notes from the metadata sidecar) at their
    /// citation refs. Session-level notes render once near the top; turn-level
    /// notes render after the message they're attached to. Notes stay marked
    /// "private annotation" so consumers don't conflate them with session
    /// content. No-op when the metadata sidecar is absent or has no matching
    /// notes.
    #[arg(long, conflicts_with = "params")]
    pub(crate) include_notes: bool,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{format, session, output?, turn_range?, include_notes?}`.
    /// Lets agents skip per-flag discovery and submit a single JSON request.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

#[derive(Args)]
pub(crate) struct IndexCommand {
    /// Reindex only sessions from this provider
    /// (`claude-code`, `copilot-cli`, `gemini-cli`, `codex-cli`, `opencode`, `cursor`).
    #[arg(long, value_parser = parse_provider_slug, conflicts_with = "params")]
    pub(crate) provider: Option<Provider>,

    /// Force a full rebuild by clearing the index first.
    #[arg(long, conflicts_with = "params")]
    pub(crate) force: bool,

    /// Authorise the one-off download of the embedding model
    /// (~90 MB `AllMiniLML6V2`). Required the first time semantic indexing
    /// runs; consent is persisted next to the index, so subsequent runs
    /// don't need this flag. Without consent (and without this flag),
    /// indexing stays purely lexical.
    #[arg(long, conflicts_with = "params")]
    pub(crate) accept_download: bool,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{provider?, force?, accept_download?}`.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

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
    #[arg(long, value_name = "PATH", conflicts_with_all = ["stdin", "params"])]
    pub(crate) query_file: Option<PathBuf>,

    /// Read the query from standard input (read until EOF).
    ///
    /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
    /// without escaping. Trailing whitespace is stripped.
    #[arg(long, conflicts_with = "params")]
    pub(crate) stdin: bool,

    /// Maximum number of hits to return
    #[arg(long, short = 'n', default_value_t = 20, conflicts_with = "params")]
    pub(crate) limit: usize,

    /// Opaque pagination cursor from a prior `meta.next_cursor`.
    #[arg(long)]
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
        default_value_t = 2000,
        value_name = "MS",
        conflicts_with = "params"
    )]
    pub(crate) watch_interval_ms: u64,

    /// Stop watch mode after N polls (0 = run until interrupted; default 0).
    ///
    /// Mostly useful for tests and one-shot snapshots.
    #[arg(long, default_value_t = 0, value_name = "N", conflicts_with = "params")]
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
        default_value_t = 0.0,
        value_name = "FLOAT",
        conflicts_with = "params"
    )]
    pub(crate) hybrid_weight: f32,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{query, limit?, json?, hybrid_weight?}`.
    /// The `query` field carries the literal query string; use
    /// `--query-file` / `--stdin` for file/stdin input.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

#[derive(Args)]
pub(crate) struct ShowCommand {
    /// Citation ref. E.g. `claude-code/abc-123#7`.
    #[arg(
        value_name = "REF",
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) reference: Option<String>,

    /// Output format: md (default), json, text.
    #[arg(long, short, default_value = "md", conflicts_with = "params")]
    pub(crate) format: ShowFormat,

    /// Include N turns before and after the target for context (default 0).
    #[arg(long, default_value_t = 0, conflicts_with = "params")]
    pub(crate) include_context: u32,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{reference, format?, include_context?}`.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

#[derive(Args)]
pub(crate) struct DiffCommand {
    /// First session ref (e.g. `claude-code/abc-123` or `laptop:claude-code/abc-123`).
    #[arg(value_name = "SESSION1")]
    pub(crate) session1: String,

    /// Second session ref (e.g. `claude-code/def-456` or `laptop:claude-code/def-456`).
    #[arg(value_name = "SESSION2")]
    pub(crate) session2: String,

    /// Context lines around each changed hunk (default 2).
    #[arg(long, short = 'c', default_value_t = 2, value_name = "N")]
    pub(crate) context: usize,

    /// Force JSON output (default: unified diff text on TTY, JSON on pipe).
    #[arg(long)]
    pub(crate) json: bool,
}
