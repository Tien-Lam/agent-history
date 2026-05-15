use std::path::PathBuf;

use aghist::model::Provider;
use aghist::todos::TodoKind;
use aghist::{config, export};
use clap::Subcommand;

use super::resolvers::{
    parse_provider_slug, parse_todo_kind, parse_transport, parse_usage_group_by, ShowFormat,
};

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Export a session to Markdown, JSON, or HTML
    Export {
        /// Output format: md, json, html
        #[arg(
            long,
            short,
            conflicts_with = "params",
            required_unless_present = "params"
        )]
        format: Option<export::ExportFormat>,

        /// Session ID (or prefix) to export
        #[arg(
            long,
            short,
            conflicts_with = "params",
            required_unless_present = "params"
        )]
        session: Option<String>,

        /// Output file path (defaults to stdout)
        #[arg(long, short, conflicts_with = "params")]
        output: Option<PathBuf>,

        /// Slice the session by 1-based turn range (e.g. `12:25`, `:10`, `5:`, or `7`).
        /// Bounds are inclusive. Out-of-range bounds clamp to the available messages.
        #[arg(long, conflicts_with = "params")]
        turn_range: Option<String>,

        /// Inline private annotations (notes from the metadata sidecar) at their
        /// citation refs. Session-level notes render once near the top; turn-level
        /// notes render after the message they're attached to. Notes stay marked
        /// "private annotation" so consumers don't conflate them with session
        /// content. No-op when the metadata sidecar is absent or has no matching
        /// notes.
        #[arg(long, conflicts_with = "params")]
        include_notes: bool,

        /// JSON request body containing all params at once. Mutually exclusive
        /// with other flags. Schema: `{format, session, output?, turn_range?, include_notes?}`.
        /// Lets agents skip per-flag discovery and submit a single JSON request.
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// Build or refresh the search index. Idempotent and delta-aware.
    ///
    /// Skips sessions whose source files are unchanged since the last run,
    /// re-indexes those that have changed, and indexes any new sessions.
    /// Always exits with status 0 on success and prints a JSON summary
    /// of `added` / `updated` / `unchanged` counts to stdout.
    Index {
        /// Reindex only sessions from this provider
        /// (`claude-code`, `copilot-cli`, `gemini-cli`, `codex-cli`, `opencode`, `cursor`).
        #[arg(long, value_parser = parse_provider_slug, conflicts_with = "params")]
        provider: Option<Provider>,

        /// Force a full rebuild by clearing the index first.
        #[arg(long, conflicts_with = "params")]
        force: bool,

        /// Authorise the one-off download of the embedding model
        /// (~90 MB `AllMiniLML6V2`). Required the first time semantic indexing
        /// runs; consent is persisted next to the index, so subsequent runs
        /// don't need this flag. Without consent (and without this flag),
        /// indexing stays purely lexical.
        #[arg(long, conflicts_with = "params")]
        accept_download: bool,

        /// JSON request body containing all params at once. Mutually exclusive
        /// with other flags. Schema: `{provider?, force?, accept_download?}`.
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// Search indexed sessions for a query
    Search {
        /// Tantivy query string (matches content + project fields).
        ///
        /// Omit when reading the query from `--query-file`, `--stdin`, or `--params`.
        #[arg(conflicts_with_all = ["query_file", "stdin", "params"])]
        query: Option<String>,

        /// Read the query from a file (use `-` for stdin).
        ///
        /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
        /// without escaping. Trailing whitespace is stripped.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["stdin", "params"])]
        query_file: Option<PathBuf>,

        /// Read the query from standard input (read until EOF).
        ///
        /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
        /// without escaping. Trailing whitespace is stripped.
        #[arg(long, conflicts_with = "params")]
        stdin: bool,

        /// Maximum number of hits to return
        #[arg(long, short = 'n', default_value_t = 20, conflicts_with = "params")]
        limit: usize,

        /// Opaque pagination cursor from a prior `meta.next_cursor`.
        #[arg(long)]
        cursor: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY)
        #[arg(long, conflicts_with = "params")]
        json: bool,

        /// Long-running stream: emit one NDJSON line per new hit as sessions land.
        ///
        /// First poll backfills all existing matches up to `--limit`, then each
        /// subsequent poll emits only previously-unseen `(session_id, message_id)`
        /// hits. Useful for an "agent of agents" watching another agent's progress.
        /// Output is NDJSON regardless of TTY; `--json` is implied.
        #[arg(long, conflicts_with = "params")]
        watch: bool,

        /// Poll interval in milliseconds when `--watch` is set (default 2000).
        #[arg(
            long,
            default_value_t = 2000,
            value_name = "MS",
            conflicts_with = "params"
        )]
        watch_interval_ms: u64,

        /// Stop watch mode after N polls (0 = run until interrupted; default 0).
        ///
        /// Mostly useful for tests and one-shot snapshots.
        #[arg(long, default_value_t = 0, value_name = "N", conflicts_with = "params")]
        watch_iterations: u32,

        /// Show BM25 score breakdown per result (Tantivy explanation tree).
        /// Useful for tuning relevance and surfacing ranking surprises.
        #[arg(long, conflicts_with = "params")]
        debug_search: bool,

        /// Reciprocal Rank Fusion weight on the semantic side, in `[0.0, 1.0]`.
        ///
        /// `0.0` (default) → lexical-only BM25, identical to omitting the flag.
        /// `0.5` → equal RRF blend of BM25 and `FastEmbed` cosine ranks.
        /// `1.0` → semantic-only.
        ///
        /// Fails open: if the binary lacks the `embeddings` feature, or no
        /// consent / embedding store exists yet, the search degrades to
        /// lexical-only regardless of this value (see `meta.engine` in JSON
        /// output). When hybrid is active, `score` becomes the RRF fused
        /// score (small, ~0–0.03) — not a BM25 score.
        #[arg(
            long,
            default_value_t = 0.0,
            value_name = "FLOAT",
            conflicts_with = "params"
        )]
        hybrid_weight: f32,

        /// JSON request body containing all params at once. Mutually exclusive
        /// with other flags. Schema: `{query, limit?, json?, hybrid_weight?}`.
        /// The `query` field carries the literal query string; use
        /// `--query-file` / `--stdin` for file/stdin input.
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// Machine-readable doctor: validates index, manifest, and provider state.
    ///
    /// Exits 0 if all checks pass (or only warn), 1 if any check fails. The
    /// JSON envelope is `{ok, checks:[{name, status, hint?}], summary}` so
    /// agents can branch on individual check kinds.
    Health,
    /// Inspect detected provider sources, or manage the remote-source registry.
    ///
    /// With no subcommand: lists detected provider sources (paths, session
    /// counts, sizes, last-indexed-at) — helpful for diagnosing "why isn't
    /// my session showing up?".
    ///
    /// Subcommands manage the persistent registry of remote sources stored
    /// in `config.toml` under `[[sources]]`. Local sources are still
    /// auto-detected; the registry is for hosts whose history dirs aghist
    /// can't see directly (e.g. SSH/rsync targets on other machines).
    Sources {
        #[command(subcommand)]
        command: Option<SourcesCommand>,
    },
    /// Resolve a citation ref `<provider>/<session-id>#<turn>` to one message.
    Show {
        /// Citation ref. E.g. `claude-code/abc-123#7`.
        #[arg(
            value_name = "REF",
            conflicts_with = "params",
            required_unless_present = "params"
        )]
        reference: Option<String>,

        /// Output format: md (default), json, text.
        #[arg(long, short, default_value = "md", conflicts_with = "params")]
        format: ShowFormat,

        /// Include N turns before and after the target for context (default 0).
        #[arg(long, default_value_t = 0, conflicts_with = "params")]
        include_context: u32,

        /// JSON request body containing all params at once. Mutually exclusive
        /// with other flags. Schema: `{reference, format?, include_context?}`.
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// Compare two sessions turn-by-turn in diff-hunk style.
    ///
    /// Computes the longest-common-subsequence of the two sessions' messages
    /// (keyed by role + first-64-chars of content) and emits hunks of
    /// diverging turns, with 2 lines of shared context around each hunk.
    /// Useful for "compare yesterday's debug session with today's working one".
    ///
    /// Session refs: `<provider>/<session-id>` (no turn suffix).
    Diff {
        /// First session ref (e.g. `claude-code/abc-123`).
        #[arg(value_name = "SESSION1")]
        session1: String,

        /// Second session ref (e.g. `claude-code/def-456`).
        #[arg(value_name = "SESSION2")]
        session2: String,

        /// Context lines around each changed hunk (default 2).
        #[arg(long, short = 'c', default_value_t = 2, value_name = "N")]
        context: usize,

        /// Force JSON output (default: unified diff text on TTY, JSON on pipe).
        #[arg(long)]
        json: bool,
    },
    /// Track how a topic evolved across sessions (LLM-required).
    ///
    /// Finds sessions that mention the topic by keyword, extracts relevant
    /// excerpts, and asks the LLM what *changed* about the topic across them.
    /// Output is a chronological timeline: `{session_ref, date, event, direction}`.
    /// Directions: `introduced`, `revised`, `confirmed`, `dropped`.
    ///
    /// Requires `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
    Track {
        /// Free-text topic to track (e.g. "auth middleware", "BM25 scoring").
        #[arg(value_name = "TOPIC")]
        topic: String,

        /// Maximum sessions to scan for the topic (0 = no limit, default 50).
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,

        /// Force JSON output (default: table on TTY, JSON on pipe).
        #[arg(long)]
        json: bool,

        /// Override the LLM model id (default: from `AGHIST_LLM_MODEL` or claude-haiku-4-5).
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,
    },
    /// Heuristic-extract candidate architectural decisions from sessions.
    ///
    /// Default path is the deterministic regex/marker heuristic: scores each
    /// sentence against decision-marker phrases (e.g. "we decided", "instead
    /// of") and returns ranked candidates with citation refs.
    ///
    /// Pass `--llm` to route the heuristic candidates through a Claude
    /// Messages API call that returns structured records of the form
    /// `{summary, rationale, alternatives, ref}`. Configured via env:
    /// `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`),
    /// `AGHIST_LLM_ENDPOINT` (defaults to api.anthropic.com),
    /// `AGHIST_LLM_MODEL` (defaults to claude-haiku-4-5). The system prompt
    /// is sent with `cache_control: ephemeral` so multi-session runs reuse
    /// Anthropic's prompt cache.
    Decisions {
        /// Restrict to a single session by id, unique id prefix, or full
        /// citation ref `<provider>/<session-id>#<turn>` (turn ignored).
        #[arg(long, short = 's', value_name = "SESSION_OR_REF")]
        session: Option<String>,

        /// Drop sentences whose score is below this threshold.
        /// Default 3.0 keeps explicit decisions and pairs of soft markers.
        #[arg(long, default_value_t = aghist::decisions::DEFAULT_THRESHOLD, value_name = "FLOAT")]
        threshold: f32,

        /// Maximum number of candidates to return across all sessions,
        /// after sorting by score descending (heuristic) or by recency (--llm).
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,

        /// Route heuristic candidates through an LLM for structured extraction.
        /// Requires `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
        #[arg(long)]
        llm: bool,

        /// Override the LLM model id (default: claude-haiku-4-5-20251001
        /// or `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,
    },
    /// Run a stdio MCP server exposing aghist's read paths to agents.
    ///
    /// Speaks JSON-RPC 2.0 over stdin/stdout with newline-delimited messages,
    /// per the MCP stdio transport. Tools: `search_sessions`, `list_sessions`,
    /// `get_session`, `get_message`, `reindex`, `health`.
    Mcp,
    /// Emit JSON-Schema (draft-2020-12) for an aghist subcommand.
    ///
    /// Lets agents discover params, response shapes, and exit codes without
    /// scraping `--help`. Use `--list` to enumerate available subcommands or
    /// `--all` to dump every schema in one document.
    Schema {
        /// Subcommand name (e.g. `search`, `list`, `health`). Omit with `--list` or `--all`.
        #[arg(value_name = "SUBCMD")]
        subcommand: Option<String>,

        /// List available schema subcommand names as JSON.
        #[arg(long, conflicts_with_all = ["all", "subcommand"])]
        list: bool,

        /// Emit every schema as one object keyed by subcommand name.
        #[arg(long, conflicts_with_all = ["list", "subcommand"])]
        all: bool,
    },
    /// Surface unresolved TODOs / follow-ups / open bd refs across sessions.
    ///
    /// Heuristic-first scan: looks for `TODO`, `follow-up`, `come back to`,
    /// `we should`, and beads-style refs (e.g. `ahist-y3o.7.2`). Each match
    /// becomes one candidate keyed by a citation ref so the caller can
    /// `aghist show` or quote the originating turn. No LLM, no `bd` lookups —
    /// agents can post-process (e.g. drop refs whose `bd show` reports closed).
    ///
    /// Pass `--llm` to route the heuristic candidates through a Claude
    /// Messages API call that returns structured records of the form
    /// `{description, raised_at: ref, target_session?, status_inferred}`.
    /// Configured via env (`ANTHROPIC_API_KEY` / `AGHIST_LLM_API_KEY`,
    /// `AGHIST_LLM_ENDPOINT`, `AGHIST_LLM_MODEL`); the system prompt is sent
    /// with `cache_control: ephemeral` to cap re-invocation cost.
    Todos {
        /// Restrict to one or more kinds. Repeat the flag, or comma-separate.
        /// Valid: `todo`, `follow-up`, `come-back-to`, `we-should`, `bd-ref`.
        #[arg(long, value_delimiter = ',', value_parser = parse_todo_kind, value_name = "KIND")]
        kind: Vec<TodoKind>,

        /// Maximum number of candidates to emit (0 = no limit).
        #[arg(long, short = 'n', default_value_t = 200)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,

        /// Route heuristic candidates through an LLM for structured extraction
        /// (`description` / `target_session` / `status_inferred`). Requires
        /// `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
        #[arg(long)]
        llm: bool,

        /// Override the LLM model id (default: claude-haiku-4-5-20251001 or
        /// `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,
    },
    /// Cluster sessions into "threads" of related work.
    ///
    /// Default heuristic: bucket sessions by `project_name`, then walk each
    /// bucket chronologically — sessions cluster when their gap is within
    /// `--gap-hours` (default 4h). Useful for "what's the history on feature
    /// X across multiple sessions?".
    ///
    /// Pass `--llm` to route session digests through a Claude Messages API
    /// call that groups by semantic topic *across project boundaries*. The
    /// heuristic can't cross projects; the LLM can. Configured via env
    /// (`ANTHROPIC_API_KEY` / `AGHIST_LLM_API_KEY`, `AGHIST_LLM_ENDPOINT`,
    /// `AGHIST_LLM_MODEL`); the system prompt is sent with
    /// `cache_control: ephemeral` to cap re-invocation cost.
    Threads {
        /// Cluster gap in hours. Sessions in the same project within this gap
        /// merge into one thread; longer gaps split. Ignored with `--llm`.
        #[arg(long, default_value_t = aghist::threads::DEFAULT_GAP_HOURS, value_name = "HOURS")]
        gap_hours: i64,

        /// Drop threads with fewer than this many sessions. Ignored with `--llm`.
        #[arg(long, default_value_t = 1, value_name = "N")]
        min_sessions: usize,

        /// Maximum number of threads to emit (0 = no limit).
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,

        /// Route session digests through an LLM for semantic topic clustering
        /// across project boundaries. Requires `ANTHROPIC_API_KEY` (or
        /// `AGHIST_LLM_API_KEY`).
        #[arg(long)]
        llm: bool,

        /// Override the LLM model id (default: claude-haiku-4-5-20251001 or
        /// `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,

        /// Cap on session digests sent to the LLM (most recent kept). One
        /// digest is ~150 bytes, so 200 ≈ 7.5K input tokens per call. Only
        /// meaningful with `--llm`.
        #[arg(long, default_value_t = 200, value_name = "N")]
        llm_max_sessions: usize,
    },
    /// Manage per-user notes attached to sessions or turns.
    ///
    /// Notes live in the metadata sidecar (`~/.local/share/aghist/metadata.db`
    /// by default; override with `AGHIST_METADATA_DB`). Each note is keyed by a
    /// session ref of the form `<provider>/<session-id>` (session-level) or
    /// `<provider>/<session-id>#<turn>` (turn-level). aghist never mutates the
    /// underlying provider history files.
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Manage per-user tags attached to sessions or turns.
    ///
    /// Tags live in the metadata sidecar (same database as `aghist note`).
    /// Each tag is a short label (e.g. `review`, `todo`) attached to a session
    /// ref of the form `<provider>/<session-id>` or
    /// `<provider>/<session-id>#<turn>`. The (`session_ref`, `tag`) pair is unique:
    /// adding the same tag twice is a no-op error. aghist never mutates the
    /// underlying provider history files.
    Tag {
        #[command(subcommand)]
        command: TagCommand,
    },
    /// Mark a session or turn as starred.
    ///
    /// Stars live in the metadata sidecar (same database as `aghist note`/`tag`).
    /// Each star is keyed by a session ref of the form `<provider>/<session-id>`
    /// or `<provider>/<session-id>#<turn>`. Starring an already-starred ref
    /// raises a `star-conflict` error. aghist never mutates provider history files.
    Star {
        /// Session ref: `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
        #[arg(value_name = "REF")]
        reference: String,
    },
    /// Remove a star from a session or turn. Errors with `star-not-found` if
    /// the ref is not currently starred.
    Unstar {
        /// Session ref: `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
        #[arg(value_name = "REF")]
        reference: String,
    },
    /// List starred sessions and turns.
    ///
    /// With no ref: every star, newest first. With `<provider>/<session-id>`:
    /// the session row plus any of its turns. With a turn-level ref: that turn
    /// exactly. Empty result exits with code 3.
    Stars {
        /// Optional session ref filter.
        #[arg(value_name = "REF")]
        reference: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Aggregate token usage and (when pricing is known) USD cost across sessions.
    ///
    /// Walks every session that passes the global filter flags (`--provider`,
    /// `--since`, `--project`, etc.) and produces one row per group (default
    /// `--by model`). Pricing comes from a small hand-curated table — sessions
    /// using a model not in the table contribute tokens but report `cost_usd:
    /// null`, and any unpriced session anywhere in the report nulls the
    /// overall total too. We don't extrapolate prices.
    ///
    /// JSON envelope: `{rows:[...], totals:{...}, meta:{group_by, ...}}`.
    /// Rows are ordered by `total_tokens` descending, with key as a stable
    /// tiebreaker. Empty result exits 3.
    Usage {
        /// Group rows by `model` (default), `provider`, or `project`.
        #[arg(long, default_value = "model", value_parser = parse_usage_group_by, value_name = "DIM")]
        by: aghist::usage::GroupBy,

        /// Cap rows after sorting (0 = no limit). Totals always cover every
        /// matching session, even those clipped from `rows`.
        #[arg(long, short = 'n', default_value_t = 0)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Per-project productivity dashboard.
    ///
    /// Aggregates one project's history into a single envelope: session and
    /// message counts, token usage (with cost when known), heuristic
    /// architectural decisions, open TODOs, cross-session work threads, the
    /// files most often touched by tool calls, and a 24-bucket UTC
    /// time-of-day histogram. No LLM — agents that want richer extraction can
    /// post-process by `aghist show`-ing the citation refs.
    ///
    /// `<name>` is matched as a case-insensitive substring against the
    /// session's `project_name`. Use `--decisions`/`--todos`/`--threads`/
    /// `--files` to cap each section (raw counts live in `meta.*_total`).
    ///
    /// Empty result (no matching sessions) exits with code 3.
    Project {
        /// Project name. Matched as a case-insensitive substring against
        /// each session's `project_name`.
        #[arg(value_name = "NAME")]
        name: String,

        /// Cap the decisions section. 0 = no cap.
        #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.decisions, value_name = "N")]
        decisions: usize,

        /// Cap the todos section. 0 = no cap.
        #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.todos, value_name = "N")]
        todos: usize,

        /// Cap the threads section. 0 = no cap.
        #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.threads, value_name = "N")]
        threads: usize,

        /// Cap the top-files section. 0 = no cap.
        #[arg(long, default_value_t = aghist::project::ProjectLimits::DEFAULTS.files, value_name = "N")]
        files: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Cross-project weekly summary suitable for journals or reviews.
    ///
    /// Aggregates a window of activity across every provider into a single
    /// envelope: top active projects, decision count, open TODOs, and
    /// completed work threads. Token totals and (when pricing is known)
    /// USD cost line up with `aghist usage`. The window defaults to the
    /// last 7 days; `--week`/`--month`/`--days N` are mutually exclusive
    /// shortcuts. The global `--since`/`--until` filters override the
    /// computed start/end if set.
    ///
    /// Default output is Markdown — paste straight into a journal. Pass
    /// `--json` for the structured envelope (schema: `aghist schema report`).
    /// Empty result (no matching sessions) exits with code 3.
    Report {
        /// Window length in days. Mutually exclusive with `--week`/`--month`.
        #[arg(long, value_name = "N", conflicts_with_all = ["week", "month"])]
        days: Option<i64>,

        /// Shorthand for `--days 7`.
        #[arg(long, conflicts_with_all = ["days", "month"])]
        week: bool,

        /// Shorthand for `--days 30`.
        #[arg(long, conflicts_with_all = ["days", "week"])]
        month: bool,

        /// Cap the top-projects section. 0 = no cap.
        #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.top_projects, value_name = "N")]
        top_projects: usize,

        /// Cap the decisions section. 0 = no cap.
        #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.decisions, value_name = "N")]
        decisions: usize,

        /// Cap the todos section. 0 = no cap.
        #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.todos, value_name = "N")]
        todos: usize,

        /// Cap the threads section. 0 = no cap.
        #[arg(long, default_value_t = aghist::report::ReportLimits::DEFAULTS.threads, value_name = "N")]
        threads: usize,

        /// Emit the structured JSON envelope instead of Markdown.
        #[arg(long)]
        json: bool,
    },
    /// Update aghist to the latest release
    Update,
    /// Remove aghist binary and data
    Uninstall,
}

/// Subcommands of `aghist sources` that manage the remote-source registry.
#[derive(Subcommand)]
pub(crate) enum SourcesCommand {
    /// Register a new remote source. Persists to `config.toml`.
    Add {
        /// Stable identifier for the source (used by `remove`).
        #[arg(value_name = "NAME")]
        name: String,
        /// Hostname or `user@host` pointing at the remote machine.
        #[arg(long, value_name = "HOST")]
        host: String,
        /// Path on the remote machine where the agent history lives.
        #[arg(long, value_name = "PATH")]
        path: String,
        /// Transport used to reach the remote (`ssh` or `rsync`). Defaults to `ssh`.
        #[arg(long, default_value = "ssh", value_parser = parse_transport, value_name = "TRANSPORT")]
        transport: config::Transport,
    },
    /// List registered remote sources.
    List,
    /// Remove a registered remote source by name.
    Remove {
        /// Name of the source to remove (matches `add --name`).
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Pull a remote source's history into a local cache via rsync.
    ///
    /// Mirrors `<host>:<path>/` to `<cache>/sources/<name>/data/` using
    /// rsync. The cache root defaults to the platform cache dir (overridable
    /// via `AGHIST_SOURCES_CACHE_DIR`). After pulling, writes a per-source
    /// manifest with byte/file counts and the pull timestamp; downstream
    /// indexing (federated search, ahist-y3o.6.3) consumes these.
    ///
    /// `--all` pulls every registered source in turn. Pass `--dry-run` to
    /// invoke rsync with `--dry-run` (no files written) — useful to validate
    /// connectivity without mutating the cache. The rsync binary can be
    /// overridden with `AGHIST_RSYNC_BIN` (used by tests; not for end users).
    Pull {
        /// Name of the source to pull. Mutually exclusive with `--all`.
        #[arg(value_name = "NAME", conflicts_with = "all")]
        name: Option<String>,

        /// Pull every registered source.
        #[arg(long, conflicts_with = "name")]
        all: bool,

        /// Run rsync with `--dry-run`; no files are written.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Subcommands of `aghist note` that manage per-user session annotations.
#[derive(Subcommand)]
pub(crate) enum NoteCommand {
    /// Attach a new note to a session ref. The body is read from `--body`,
    /// `--body-file`, or stdin (`--stdin`). Outputs the created note as JSON
    /// (single object) on stdout.
    Add {
        /// Session ref: `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
        #[arg(value_name = "REF")]
        reference: String,

        /// Note body as a literal string. Mutually exclusive with `--body-file`/`--stdin`.
        #[arg(long, conflicts_with_all = ["body_file", "stdin"], value_name = "TEXT")]
        body: Option<String>,

        /// Read the body from a file (use `-` for stdin).
        #[arg(long, conflicts_with_all = ["body", "stdin"], value_name = "PATH")]
        body_file: Option<PathBuf>,

        /// Read the body from standard input (read until EOF).
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        stdin: bool,
    },
    /// List notes, optionally filtered by session ref.
    ///
    /// With no ref: every note, newest first. With `<provider>/<session-id>`:
    /// every note on that session and any of its turns. With a turn-level ref:
    /// only notes on that exact turn.
    List {
        /// Optional session ref filter.
        #[arg(value_name = "REF")]
        reference: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Replace the body of an existing note.
    Edit {
        /// Numeric note id (from `aghist note add` or `aghist note list`).
        #[arg(value_name = "ID")]
        id: i64,

        /// New body as a literal string. Mutually exclusive with `--body-file`/`--stdin`.
        #[arg(long, conflicts_with_all = ["body_file", "stdin"], value_name = "TEXT")]
        body: Option<String>,

        /// Read the new body from a file (use `-` for stdin).
        #[arg(long, conflicts_with_all = ["body", "stdin"], value_name = "PATH")]
        body_file: Option<PathBuf>,

        /// Read the new body from standard input (read until EOF).
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        stdin: bool,
    },
    /// Remove a note by id. Outputs the deleted row as JSON on stdout.
    #[command(alias = "rm")]
    Remove {
        /// Numeric note id.
        #[arg(value_name = "ID")]
        id: i64,
    },
}

/// Subcommands of `aghist tag` that manage per-user session tags.
#[derive(Subcommand)]
pub(crate) enum TagCommand {
    /// Attach a tag to a session ref. Outputs the created row as JSON on stdout.
    /// Adding the same (ref, tag) pair twice raises a `tag-conflict` error.
    Add {
        /// Session ref: `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
        #[arg(value_name = "REF")]
        reference: String,

        /// Tag label. Whitespace-trimmed; must be non-empty.
        #[arg(value_name = "TAG")]
        tag: String,
    },
    /// List tags, optionally filtered by session ref and/or tag value.
    ///
    /// With no arguments: every tag, newest first. With `<provider>/<session-id>`:
    /// every tag on that session and any of its turns. With a turn-level ref:
    /// tags on that exact turn. `--tag <name>` narrows to a specific tag value
    /// (combinable with the ref filter).
    List {
        /// Optional session ref filter.
        #[arg(value_name = "REF")]
        reference: Option<String>,

        /// Filter by exact tag value (e.g. `--tag review`).
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Detach a tag from a session ref. Outputs the deleted row as JSON.
    #[command(alias = "rm")]
    Remove {
        /// Session ref the tag is attached to.
        #[arg(value_name = "REF")]
        reference: String,

        /// Tag label to remove.
        #[arg(value_name = "TAG")]
        tag: String,
    },
}
