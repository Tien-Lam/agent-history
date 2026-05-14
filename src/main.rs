use aghist::cli_error::{
    ErrorEnvelope, EXIT_EMPTY, EXIT_ERROR, EXIT_OK, EXIT_USAGE,
};
use aghist::model::{CitationRef, ContentBlock, Message, Provider, Role, Session};
use aghist::output::{CommandKind, OutputMode};
use aghist::health::{self, HealthCheck, HealthStatus};
#[cfg(feature = "embeddings")]
use aghist::embed;
use aghist::search::SearchFilters;
use aghist::todos::{self, TodoCandidate, TodoKind};
use aghist::metadata::{self, MetadataError, Note, Star, Tag};
use aghist::{app, config, export, federated, mcp, provider, schema, search};

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser)]
#[command(name = "aghist", version, about = "Browse and search AI agent conversation history")]
#[allow(clippy::struct_excessive_bools)] // CLI flag struct: clap requires bool fields per flag
struct Cli {
    /// List sessions without opening the TUI
    #[arg(long)]
    list: bool,

    /// Maximum number of sessions to return when paired with `--list`.
    /// JSON output includes `meta.next_cursor` if more results remain.
    #[arg(long, default_value_t = 20, requires = "list")]
    limit: usize,

    /// Opaque pagination cursor (from a prior `meta.next_cursor`) for `--list`.
    #[arg(long, requires = "list")]
    cursor: Option<String>,

    /// Force rebuild the search index
    #[arg(long)]
    reindex: bool,

    /// Force JSON output (for one-shot commands like --list, export).
    /// Mutually exclusive with --ndjson.
    #[arg(long, global = true)]
    json: bool,

    /// Force newline-delimited JSON output (for streaming commands).
    /// Mutually exclusive with --json.
    #[arg(long, global = true)]
    ndjson: bool,

    #[command(flatten)]
    filters: FilterArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

// Common filter flags shared between `--list` and `search`.
//
// `--has-tool-call` filters at the message level (drops messages without a
// tool invocation); other flags filter at the session or message level
// depending on the subcommand. `--since`/`--until` accept RFC 3339 dates only.
//
// Doc comment intentionally suppressed: clap promotes a flattened struct's
// doc comment to the parent's `about` text, overriding our explicit
// `about = "Browse and search..."` on `Cli`.
#[derive(Debug, Clone, Args)]
struct FilterArgs {
    /// Restrict to a single provider (`claude-code`, `copilot-cli`,
    /// `gemini-cli`, `codex-cli`, `opencode`).
    #[arg(long, global = true, value_parser = parse_provider_slug, value_name = "SLUG")]
    provider: Option<Provider>,

    /// RFC 3339 lower bound on message/session timestamp (inclusive).
    /// Example: `--since 2025-01-01T00:00:00Z`.
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    since: Option<DateTime<Utc>>,

    /// RFC 3339 upper bound on message/session timestamp (inclusive).
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    until: Option<DateTime<Utc>>,

    /// Substring match against the session's project name (case-insensitive).
    #[arg(long, global = true, value_name = "NAME")]
    project: Option<String>,

    /// Restrict to messages with this role: `user`, `assistant`, or `tool`.
    #[arg(long, global = true, value_parser = parse_role_slug, value_name = "ROLE")]
    role: Option<Role>,

    /// Keep only messages (or sessions containing messages) that include a
    /// tool invocation. Has no effect on session-level lookups that do not
    /// load message content.
    #[arg(long, global = true)]
    has_tool_call: bool,

    /// Keep only sessions that have a user note whose body contains this
    /// substring (case-insensitive). Matches notes attached to the session
    /// itself or to any of its turns. Notes live in the metadata sidecar
    /// (`~/.local/share/aghist/metadata.db`; `AGHIST_METADATA_DB` overrides).
    #[arg(long, global = true, value_name = "SUBSTR")]
    note: Option<String>,

    /// Keep only sessions that have this exact tag attached (session-level
    /// OR on any of its turns). Tags live in the same metadata sidecar.
    #[arg(long, global = true, value_name = "NAME")]
    tag: Option<String>,

    /// Keep only sessions that have at least one star (session-level OR on
    /// any of its turns). Stars live in the same metadata sidecar.
    #[arg(long, global = true)]
    starred: bool,
}

impl FilterArgs {
    fn to_search_filters(&self) -> SearchFilters {
        SearchFilters {
            provider: self.provider,
            since: self.since,
            until: self.until,
            project: self.project.clone(),
            role: self.role,
            has_tool_call: self.has_tool_call,
        }
    }

    fn has_metadata_filter(&self) -> bool {
        self.note.is_some() || self.tag.is_some() || self.starred
    }
}

fn parse_role_slug(raw: &str) -> Result<Role, String> {
    Role::from_slug(raw).ok_or_else(|| {
        format!("unknown role '{raw}'. Valid: user, assistant, tool")
    })
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC 3339 timestamp '{raw}': {e}"))
}

#[derive(Subcommand)]
enum Command {
    /// Export a session to Markdown, JSON, or HTML
    Export {
        /// Output format: md, json, html
        #[arg(long, short, conflicts_with = "params", required_unless_present = "params")]
        format: Option<export::ExportFormat>,

        /// Session ID (or prefix) to export
        #[arg(long, short, conflicts_with = "params", required_unless_present = "params")]
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
        #[arg(long, default_value_t = 2000, value_name = "MS", conflicts_with = "params")]
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
        #[arg(long, default_value_t = 0.0, value_name = "FLOAT", conflicts_with = "params")]
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
        #[arg(value_name = "REF", conflicts_with = "params", required_unless_present = "params")]
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
        /// (description / target_session / status_inferred). Requires
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
enum SourcesCommand {
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
enum NoteCommand {
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
enum TagCommand {
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

fn parse_transport(raw: &str) -> Result<config::Transport, String> {
    config::Transport::from_slug(raw)
        .ok_or_else(|| format!("unknown transport '{raw}'. Valid: ssh, rsync"))
}

/// JSON `--params` body for `aghist export`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    format: String,
    session: String,
    #[serde(default)]
    output: Option<PathBuf>,
    #[serde(default)]
    turn_range: Option<String>,
    #[serde(default)]
    include_notes: bool,
}

/// JSON `--params` body for `aghist index`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexParams {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    accept_download: bool,
}

/// JSON `--params` body for `aghist search`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    query: String,
    #[serde(default = "SearchParams::default_limit")]
    limit: usize,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    json: bool,
    #[serde(default)]
    hybrid_weight: f32,
}

impl SearchParams {
    fn default_limit() -> usize {
        20
    }
}

/// JSON `--params` body for `aghist show`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ShowParams {
    reference: String,
    #[serde(default = "ShowParams::default_format")]
    format: String,
    #[serde(default)]
    include_context: u32,
}

impl ShowParams {
    fn default_format() -> String {
        "md".to_string()
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(
    json: &str,
    cmd: &str,
) -> Result<T, ErrorEnvelope> {
    serde_json::from_str(json).map_err(|e| {
        ErrorEnvelope::new(
            "usage",
            format!("--params for `{cmd}` is not valid JSON: {e}"),
        )
        .with_hint("Pass a JSON object matching the subcommand schema.")
    })
}

fn parse_params_field<T, E: std::fmt::Display>(
    raw: &str,
    field: &str,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, ErrorEnvelope> {
    parse(raw).map_err(|e| {
        ErrorEnvelope::new(
            "usage",
            format!("--params field `{field}` is invalid: {e}"),
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowFormat {
    Md,
    Json,
    Text,
}

impl std::str::FromStr for ShowFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Md),
            "json" => Ok(Self::Json),
            "text" | "txt" => Ok(Self::Text),
            _ => Err(format!("unknown format '{s}' (expected: md, json, text)")),
        }
    }
}

fn parse_todo_kind(raw: &str) -> Result<TodoKind, String> {
    TodoKind::from_slug(raw).ok_or_else(|| {
        format!(
            "unknown todo kind '{raw}'. Valid: todo, follow-up, come-back-to, we-should, bd-ref"
        )
    })
}

fn parse_provider_slug(raw: &str) -> Result<Provider, String> {
    Provider::from_slug(raw).ok_or_else(|| {
        format!(
            "unknown provider slug '{raw}'. Valid: claude-code, copilot-cli, gemini-cli, codex-cli, opencode, cursor"
        )
    })
}

fn parse_usage_group_by(raw: &str) -> Result<aghist::usage::GroupBy, String> {
    aghist::usage::GroupBy::parse(raw)
        .map_err(|bad| format!("unknown --by value '{bad}'. Valid: model, provider, project"))
}

struct ResolvedExport {
    format: export::ExportFormat,
    session: String,
    output: Option<PathBuf>,
    turn_range: Option<String>,
    include_notes: bool,
}

fn resolve_export_args(
    format: Option<export::ExportFormat>,
    session: Option<String>,
    output: Option<PathBuf>,
    turn_range: Option<String>,
    include_notes: bool,
    params: Option<String>,
) -> Result<ResolvedExport, ErrorEnvelope> {
    if let Some(json) = params {
        let p: ExportParams = parse_params(&json, "export")?;
        let format = parse_params_field(&p.format, "format", str::parse::<export::ExportFormat>)?;
        Ok(ResolvedExport {
            format,
            session: p.session,
            output: p.output,
            turn_range: p.turn_range,
            include_notes: p.include_notes,
        })
    } else {
        // clap enforces these via `required_unless_present = "params"`.
        Ok(ResolvedExport {
            format: format.expect("clap requires --format unless --params is set"),
            session: session.expect("clap requires --session unless --params is set"),
            output,
            turn_range,
            include_notes,
        })
    }
}

fn resolve_index_args(
    provider: Option<Provider>,
    force: bool,
    accept_download: bool,
    params: Option<String>,
) -> Result<(Option<Provider>, bool, bool), ErrorEnvelope> {
    if let Some(json) = params {
        let p: IndexParams = parse_params(&json, "index")?;
        let provider = match p.provider {
            Some(slug) => Some(parse_params_field(&slug, "provider", parse_provider_slug)?),
            None => None,
        };
        Ok((provider, p.force, p.accept_download))
    } else {
        Ok((provider, force, accept_download))
    }
}

struct SearchArgs {
    query: Option<String>,
    query_file: Option<PathBuf>,
    stdin: bool,
    limit: usize,
    cursor: Option<String>,
    json: bool,
    hybrid_weight: f32,
}

#[allow(clippy::too_many_arguments)]
fn resolve_search_args(
    query: Option<String>,
    query_file: Option<PathBuf>,
    stdin: bool,
    limit: usize,
    cursor: Option<String>,
    json: bool,
    hybrid_weight: f32,
    params: Option<String>,
) -> Result<SearchArgs, ErrorEnvelope> {
    if let Some(raw) = params {
        let p: SearchParams = parse_params(&raw, "search")?;
        Ok(SearchArgs {
            query: Some(p.query),
            query_file: None,
            stdin: false,
            limit: p.limit,
            cursor: p.cursor,
            json: p.json,
            hybrid_weight: p.hybrid_weight,
        })
    } else {
        Ok(SearchArgs {
            query,
            query_file,
            stdin,
            limit,
            cursor,
            json,
            hybrid_weight,
        })
    }
}

fn resolve_show_args(
    reference: Option<String>,
    format: ShowFormat,
    include_context: u32,
    params: Option<String>,
) -> Result<(String, ShowFormat, u32), ErrorEnvelope> {
    if let Some(json) = params {
        let p: ShowParams = parse_params(&json, "show")?;
        let format = parse_params_field(&p.format, "format", str::parse::<ShowFormat>)?;
        Ok((p.reference, format, p.include_context))
    } else {
        Ok((
            reference.expect("clap requires REF unless --params is set"),
            format,
            include_context,
        ))
    }
}

/// Parse a 1-based inclusive turn range against a session of `total` messages.
///
/// Accepts `A:B`, `:B`, `A:`, or a bare `A`. Empty halves default to the
/// session bounds (`1` and `total`). Bounds are clamped to the available
/// range so callers can do `--turn-range :999` without failing.
///
/// Returns `(start, end)` with `1 <= start <= end <= total`, ready to be
/// converted to a 0-based half-open slice via `start-1 .. end`.
fn parse_turn_range(spec: &str, total: usize) -> Result<(usize, usize), String> {
    if total == 0 {
        return Err("session has no messages to slice".to_string());
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(format!("empty turn range '{spec}'"));
    }

    let parse_bound = |s: &str, label: &str| -> Result<Option<usize>, String> {
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<usize>()
            .map(Some)
            .map_err(|_| format!("invalid {label} '{s}' in turn range '{spec}'"))
    };

    let (start_raw, end_raw) = if let Some((a, b)) = trimmed.split_once(':') {
        (parse_bound(a, "start")?, parse_bound(b, "end")?)
    } else {
        let n = parse_bound(trimmed, "turn")?
            .ok_or_else(|| format!("empty turn range '{spec}'"))?;
        (Some(n), Some(n))
    };

    if start_raw == Some(0) || end_raw == Some(0) {
        return Err(format!("turn range '{spec}' uses 0 (turns are 1-based)"));
    }

    let start = start_raw.unwrap_or(1);
    let end = end_raw.unwrap_or(total).min(total);

    if start > total {
        return Err(format!(
            "turn range '{spec}' starts at {start} but session only has {total} message(s)"
        ));
    }
    if end < start {
        return Err(format!(
            "turn range '{spec}' has end ({end}) before start ({start})"
        ));
    }
    Ok((start, end))
}

#[cfg(test)]
mod turn_range_tests {
    use super::parse_turn_range;

    #[test]
    fn full_range() {
        assert_eq!(parse_turn_range("3:7", 10).unwrap(), (3, 7));
    }

    #[test]
    fn open_start_defaults_to_one() {
        assert_eq!(parse_turn_range(":5", 10).unwrap(), (1, 5));
    }

    #[test]
    fn open_end_defaults_to_total() {
        assert_eq!(parse_turn_range("4:", 10).unwrap(), (4, 10));
    }

    #[test]
    fn single_turn() {
        assert_eq!(parse_turn_range("7", 10).unwrap(), (7, 7));
    }

    #[test]
    fn end_clamps_to_total() {
        assert_eq!(parse_turn_range("3:999", 10).unwrap(), (3, 10));
    }

    #[test]
    fn empty_session_rejects() {
        assert!(parse_turn_range("1:1", 0).is_err());
    }

    #[test]
    fn zero_rejected() {
        assert!(parse_turn_range("0:5", 10).is_err());
        assert!(parse_turn_range("3:0", 10).is_err());
        assert!(parse_turn_range("0", 10).is_err());
    }

    #[test]
    fn start_past_end_rejects() {
        assert!(parse_turn_range("8:3", 10).is_err());
    }

    #[test]
    fn start_past_session_rejects() {
        assert!(parse_turn_range("99:100", 10).is_err());
    }

    #[test]
    fn non_numeric_rejects() {
        assert!(parse_turn_range("a:b", 10).is_err());
        assert!(parse_turn_range("abc", 10).is_err());
    }

    #[test]
    fn empty_spec_rejects() {
        assert!(parse_turn_range("", 10).is_err());
    }
}

fn init_tracing() {
    // Log to ~/.aghist/aghist.log — safe for TUI since it doesn't touch stdout/stderr
    let log_dir = directories::BaseDirs::new()
        .map_or_else(|| PathBuf::from("."), |d| d.home_dir().join(".aghist"));
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "aghist.log");
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("aghist=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .init();
}

fn main() -> ExitCode {
    init_tracing();
    color_eyre::install().ok();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return handle_clap_error(&err),
    };

    match run(cli) {
        Ok(code) => exit_code(code),
        Err(env) => {
            env.emit();
            exit_code(EXIT_ERROR)
        }
    }
}

fn handle_clap_error(err: &clap::Error) -> ExitCode {
    // Help / version output is not a failure: let clap print to stdout and
    // return success without an envelope.
    if !err.use_stderr() {
        let _ = err.print();
        return ExitCode::SUCCESS;
    }
    let message = err
        .to_string()
        .lines()
        .find(|line| !line.is_empty())
        .unwrap_or("invalid arguments")
        .trim_start_matches("error: ")
        .to_string();
    ErrorEnvelope::new("usage", message)
        .with_hint("Run `aghist --help` for usage.")
        .emit();
    exit_code(EXIT_USAGE)
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<i32, ErrorEnvelope> {
    if cli.reindex {
        let index_dir = search::SearchIndex::default_index_dir();
        if let Ok(index) = search::SearchIndex::open_or_create(&index_dir) {
            let _ = index.clear();
            eprintln!("Search index cleared. Will rebuild on next launch.");
        }
    }

    if cli.json && cli.ndjson {
        ErrorEnvelope::new("usage", "--json and --ndjson are mutually exclusive")
            .with_hint("Pick one. Without either, output auto-detects: JSON/NDJSON on a pipe, human format on a TTY.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let config = config::Config::load();
    let enabled = config.enabled_providers();
    let providers: Vec<_> = provider::detect_all_providers()
        .into_iter()
        .filter(|p| enabled.contains(&p.provider()))
        .collect();

    match cli.command {
        Some(Command::Mcp) => {
            // MCP gets a narrower view than the rest of the CLI — users can
            // hide providers from MCP clients (e.g. a personal Claude account)
            // via `providers.mcp_exposed` without disabling them locally.
            let exposed = config.mcp_exposed_providers();
            let mcp_providers: Vec<_> = providers
                .into_iter()
                .filter(|p| exposed.contains(&p.provider()))
                .collect();
            return run_mcp(mcp_providers);
        }
        Some(Command::Schema { subcommand, list, all }) => {
            return schema_command(subcommand.as_deref(), list, all);
        }
        Some(Command::Update) => return self_update(),
        Some(Command::Uninstall) => return uninstall(),
        Some(Command::Export {
            format,
            session,
            output,
            turn_range,
            include_notes,
            params,
        }) => {
            let resolved = resolve_export_args(
                format, session, output, turn_range, include_notes, params,
            )?;
            return export_session(
                &providers,
                resolved.format,
                &resolved.session,
                resolved.output.as_deref(),
                resolved.turn_range.as_deref(),
                resolved.include_notes,
            );
        }
        Some(Command::Index {
            provider,
            force,
            accept_download,
            params,
        }) => {
            let (provider, force, accept_download) =
                resolve_index_args(provider, force, accept_download, params)?;
            return run_index(&providers, provider, force, accept_download);
        }
        Some(Command::Search {
            query,
            query_file,
            stdin,
            limit,
            cursor,
            json,
            watch,
            watch_interval_ms,
            watch_iterations,
            debug_search,
            hybrid_weight,
            params,
        }) => {
            let filters = cli.filters.to_search_filters();
            let metadata_keys = resolve_metadata_filter(&cli.filters)?;
            if watch {
                return search_watch_command(
                    &providers,
                    query.as_deref(),
                    query_file.as_deref(),
                    stdin,
                    limit,
                    watch_interval_ms,
                    watch_iterations,
                    &filters,
                    metadata_keys.as_ref(),
                );
            }
            let args = resolve_search_args(
                query,
                query_file,
                stdin,
                limit,
                cursor,
                json,
                hybrid_weight,
                params,
            )?;
            return search_command(
                &providers,
                args.query.as_deref(),
                args.query_file.as_deref(),
                args.stdin,
                args.limit,
                args.cursor.as_deref(),
                args.json,
                &filters,
                debug_search,
                args.hybrid_weight,
                metadata_keys.as_ref(),
            );
        }
        Some(Command::Show {
            reference,
            format,
            include_context,
            params,
        }) => {
            let (reference, format, include_context) =
                resolve_show_args(reference, format, include_context, params)?;
            return show_command(&providers, &reference, format, include_context);
        }
        Some(Command::Diff {
            session1,
            session2,
            context,
            json,
        }) => {
            return diff_command(&providers, &session1, &session2, context, json);
        }
        Some(Command::Decisions {
            session,
            threshold,
            limit,
            json,
            llm,
            llm_model,
        }) => {
            return decisions_command(
                &providers,
                session.as_deref(),
                threshold,
                limit,
                json,
                &cli.filters,
                llm,
                llm_model.as_deref(),
            );
        }
        Some(Command::Todos {
            kind,
            limit,
            json,
            llm,
            llm_model,
        }) => {
            return todos_command(
                &providers,
                &cli.filters,
                &kind,
                limit,
                json,
                llm,
                llm_model.as_deref(),
            );
        }
        Some(Command::Threads {
            gap_hours,
            min_sessions,
            limit,
            json,
            llm,
            llm_model,
            llm_max_sessions,
        }) => {
            return threads_command(
                &providers,
                &cli.filters,
                gap_hours,
                min_sessions,
                limit,
                json,
                llm,
                llm_model.as_deref(),
                llm_max_sessions,
            );
        }
        Some(Command::Sources { command }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return match command {
                None => sources_command(&providers, mode),
                Some(SourcesCommand::List) => sources_list_remote(mode),
                Some(SourcesCommand::Add {
                    name,
                    host,
                    path,
                    transport,
                }) => sources_add_remote(&name, &host, &path, transport, mode),
                Some(SourcesCommand::Remove { name }) => sources_remove_remote(&name, mode),
                Some(SourcesCommand::Pull {
                    name,
                    all,
                    dry_run,
                }) => sources_pull_remote(name.as_deref(), all, dry_run, mode),
            };
        }
        Some(Command::Health) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return health_command(&providers, mode);
        }
        Some(Command::Note { command }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return note_dispatch(command, mode);
        }
        Some(Command::Tag { command }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return tag_dispatch(command, mode);
        }
        Some(Command::Star { reference }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return star_command(&reference, mode);
        }
        Some(Command::Unstar { reference }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return unstar_command(&reference, mode);
        }
        Some(Command::Stars { reference, json }) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            let mode = if json { OutputMode::Json } else { mode };
            return stars_list(reference.as_deref(), mode);
        }
        Some(Command::Usage { by, limit, json }) => {
            return usage_command(&providers, &cli.filters, by, limit, json);
        }
        Some(Command::Project {
            name,
            decisions,
            todos,
            threads,
            files,
            json,
        }) => {
            let limits = aghist::project::ProjectLimits {
                decisions,
                todos,
                threads,
                files,
            };
            return project_command(&providers, &cli.filters, &name, limits, json);
        }
        Some(Command::Report {
            days,
            week,
            month,
            top_projects,
            decisions,
            todos,
            threads,
            json,
        }) => {
            let window_days = if month {
                30
            } else if week {
                7
            } else {
                days.unwrap_or(7)
            };
            let limits = aghist::report::ReportLimits {
                top_projects,
                decisions,
                todos,
                threads,
            };
            return report_command(
                &providers, &cli.filters, window_days, limits, json,
            );
        }
        None => {}
    }

    if cli.list {
        let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::Streaming);
        let metadata_keys = resolve_metadata_filter(&cli.filters)?;
        return list_sessions(
            &providers,
            mode,
            cli.limit,
            cli.cursor.as_deref(),
            &cli.filters,
            metadata_keys.as_ref(),
        );
    }

    run_tui(providers, config)
}

fn run_tui(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
    config: config::Config,
) -> Result<i32, ErrorEnvelope> {
    // Install panic hook that restores the terminal before printing the panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode().map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to enable raw mode: {e}"))
    })?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to enter alternate screen: {e}"))
    })?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to construct terminal: {e}"))
    })?;

    let mut app = app::App::new(providers, config);
    let result = app.run(&mut terminal);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
        .map(|()| EXIT_OK)
        .map_err(|e| ErrorEnvelope::new("internal-error", format!("{e:#}")))
}

fn schema_command(
    subcommand: Option<&str>,
    list: bool,
    all: bool,
) -> Result<i32, ErrorEnvelope> {
    let payload = if list {
        schema::subcommand_index()
    } else if all {
        schema::all_schemas()
    } else if let Some(name) = subcommand {
        if let Some(value) = schema::schema_for(name) {
            value
        } else {
            let valid = schema::SUBCOMMANDS.join(", ");
            return Err(ErrorEnvelope::new(
                "usage",
                format!("unknown schema subcommand '{name}'"),
            )
            .with_hint(format!("Valid subcommands: {valid}")));
        }
    } else {
        ErrorEnvelope::new(
            "usage",
            "schema requires <SUBCMD>, --list, or --all",
        )
        .with_hint("Run `aghist schema --list` to see available subcommands.")
        .emit();
        return Ok(EXIT_USAGE);
    };

    serde_json::to_writer(io::stdout().lock(), &payload).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write schema output: {e}"))
    })?;
    println!();
    Ok(EXIT_OK)
}

fn run_mcp(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
) -> Result<i32, ErrorEnvelope> {
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    let server = mcp::McpServer::new(providers);
    server.serve(stdin, stdout).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("MCP server stdio error: {e}"))
    })?;
    Ok(EXIT_OK)
}

fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let started = std::time::Instant::now();

    let active: Vec<&Box<dyn provider::HistoryProvider>> = providers
        .iter()
        .filter(|p| filter.is_none_or(|want| p.provider() == want))
        .collect();

    if let Some(want) = filter {
        if active.is_empty() {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "provider '{}' is not enabled or not detected on this system",
                    want.slug()
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table)."));
        }
    }

    let mut sessions: Vec<Session> = Vec::new();
    let mut errors: Vec<(Provider, String)> = Vec::new();
    for p in &active {
        match p.discover_sessions() {
            Ok(s) => sessions.extend(s),
            Err(e) => errors.push((p.provider(), e.to_string())),
        }
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open index at {}: {e}", index_dir.display()),
        )
    })?;
    if force {
        index.clear().map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))
        })?;
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    // build_index needs the full provider list for load_messages dispatch;
    // provider filtering is enforced by only feeding it sessions from `active`.
    let stats = index.build_index(&sessions, providers, &tx).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to build index: {e}"))
    })?;

    let embed_summary = run_embeddings(&index_dir, &sessions, providers, accept_download)?;

    let provider_slugs: Vec<&'static str> = active.iter().map(|p| p.provider().slug()).collect();
    let summary = serde_json::json!({
        "providers": provider_slugs,
        "sessions_total": sessions.len(),
        "added": stats.added,
        "updated": stats.updated,
        "unchanged": stats.unchanged,
        "messages_indexed": stats.messages_indexed,
        "force": force,
        "index_dir": index_dir.display().to_string(),
        "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "errors": errors
            .iter()
            .map(|(p, msg)| serde_json::json!({ "provider": p.slug(), "error": msg }))
            .collect::<Vec<_>>(),
        "embeddings": embed_summary,
    });

    println!("{summary}");
    Ok(EXIT_OK)
}

/// Drive the (opt-in) semantic side of indexing.
///
/// Three states feed the JSON summary back to the caller:
///
/// - `disabled`: the binary was built without the `embeddings` feature, so we
///   surface that even when `--accept-download` is passed (users would
///   otherwise see silent no-ops).
/// - `awaiting-consent`: feature is compiled in, no consent file exists, and
///   `--accept-download` was not passed. Lexical indexing still happened.
/// - `enabled`: consent recorded (just now or in a prior run); embeddings
///   were generated and persisted.
// The `embeddings`-disabled variant can't fail, but the `embeddings`-enabled
// variant can — both signatures need to match so callers don't change shape.
#[cfg(not(feature = "embeddings"))]
#[allow(clippy::unnecessary_wraps)]
fn run_embeddings(
    _index_dir: &std::path::Path,
    _sessions: &[Session],
    _providers: &[Box<dyn provider::HistoryProvider>],
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    Ok(serde_json::json!({
        "status": "disabled",
        "reason": "binary built without `embeddings` feature",
        "accept_download_requested": accept_download,
    }))
}

#[cfg(feature = "embeddings")]
fn run_embeddings(
    index_dir: &std::path::Path,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    let consent = embed::Consent::load(index_dir);
    let consent = match (consent, accept_download) {
        (Some(c), _) => c,
        (None, true) => embed::Consent::record(index_dir, embed::DEFAULT_MODEL).map_err(|e| {
            ErrorEnvelope::new(
                "embed-error",
                format!("failed to record embedding-download consent: {e}"),
            )
        })?,
        (None, false) => {
            return Ok(serde_json::json!({
                "status": "awaiting-consent",
                "model": embed::DEFAULT_MODEL,
                "hint": "re-run with `--accept-download` to enable semantic indexing",
            }));
        }
    };

    let cache_dir = index_dir.join("models");
    let mut embedder = embed::Embedder::try_new(&cache_dir).map_err(|e| {
        ErrorEnvelope::new(
            "embed-error",
            format!("failed to initialise embedder: {e}"),
        )
    })?;

    // On a schema bump (STORE_VERSION mismatch), evict the old sidecar and
    // start fresh — the alternative would be to refuse to reindex, which is
    // worse UX than transparently rebuilding. We surface the eviction so it's
    // visible in the JSON summary.
    let mut evicted_old_schema = false;
    let mut store = match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(s)) => s,
        Ok(None) => embed::EmbeddingStore::create(index_dir, embedder.model_slug(), embedder.dim()),
        Err(embed::EmbedError::SchemaMismatch { .. }) => {
            embed::EmbeddingStore::evict(index_dir).map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to evict outdated embedding store: {e}"),
                )
            })?;
            evicted_old_schema = true;
            embed::EmbeddingStore::create(index_dir, embedder.model_slug(), embedder.dim())
        }
        Err(e) => {
            return Err(ErrorEnvelope::new(
                "embed-error",
                format!("failed to open embedding store: {e}"),
            ));
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut messages_embedded = 0usize;
    let mut messages_reused = 0usize;

    for session in sessions {
        let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) else {
            continue;
        };
        let messages = match provider.load_messages(session) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: {e}", session.id.0));
                continue;
            }
        };

        // (id, text, content_hash) for messages whose cached vector is stale
        // or absent. We compute the hash up front so the freshness check is a
        // cheap byte compare against what's in the store.
        let pending: Vec<(String, String, [u8; embed::HASH_LEN])> = messages
            .iter()
            .filter_map(|m| {
                let text = collect_text(m);
                if text.trim().is_empty() {
                    return None;
                }
                let hash = embed::content_hash(&text);
                if store.get_if_fresh(&m.id.0, &hash).is_some() {
                    messages_reused += 1;
                    return None;
                }
                Some((m.id.0.clone(), text, hash))
            })
            .collect();

        if pending.is_empty() {
            continue;
        }

        let texts: Vec<String> = pending.iter().map(|(_, t, _)| t.clone()).collect();
        match embedder.embed_batch(&texts) {
            Ok(vectors) => {
                for ((id, _, hash), vec) in pending.into_iter().zip(vectors) {
                    if let Err(e) = store.upsert(&id, hash, vec) {
                        errors.push(format!("{id}: {e}"));
                    } else {
                        messages_embedded += 1;
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {e}", session.id.0)),
        }
    }

    store.flush().map_err(|e| {
        ErrorEnvelope::new(
            "embed-error",
            format!("failed to persist embeddings: {e}"),
        )
    })?;

    Ok(serde_json::json!({
        "status": "enabled",
        "model": consent.model,
        "dim": store.dim(),
        "messages_embedded": messages_embedded,
        "messages_reused_from_cache": messages_reused,
        "messages_total_in_store": store.len(),
        "evicted_old_schema": evicted_old_schema,
        "consent_accepted_at": consent.accepted_at,
        "errors": errors,
    }))
}

/// Try to engage hybrid (lexical + semantic) RRF scoring.
///
/// Fail-open: returns `Ok(None)` whenever the semantic side isn't ready —
/// missing `embeddings` feature, no recorded consent, empty store, embedder
/// init failure, query embedding failure. Callers fall back to lexical-only.
/// Only "hard" errors (Tantivy failures during the fused search itself)
/// surface as `Err`.
///
/// On the success path, the embedder embeds `query`, the entire embedding
/// store is ranked by cosine similarity, the top `pool_size` candidates are
/// fed into [`search::SearchIndex::search_hybrid`], and the resulting
/// RRF-fused hits are returned.
#[cfg(not(feature = "embeddings"))]
#[allow(clippy::unnecessary_wraps)]
fn try_hybrid_search(
    _index_dir: &std::path::Path,
    _index: &search::SearchIndex,
    _query: &str,
    _pool_size: usize,
    _filters: &SearchFilters,
    _hybrid_weight: f32,
) -> Result<Option<Vec<search::SearchHit>>, ErrorEnvelope> {
    // Lean build: there's no embedder to query with, so silently fall open.
    Ok(None)
}

#[cfg(feature = "embeddings")]
fn try_hybrid_search(
    index_dir: &std::path::Path,
    index: &search::SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    hybrid_weight: f32,
) -> Result<Option<Vec<search::SearchHit>>, ErrorEnvelope> {
    if embed::Consent::load(index_dir).is_none() {
        return Ok(None);
    }
    let store = match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(s)) if !s.is_empty() => s,
        // No store, empty store, or schema-mismatched store all degrade to
        // lexical-only. SchemaMismatch is recoverable via a reindex; we don't
        // try to evict here because that's the indexer's job.
        _ => return Ok(None),
    };
    let cache_dir = index_dir.join("models");
    // Embedder init reaches the network (model download). Treat any failure
    // as a soft fail-open: the search still works lexically.
    let Ok(mut embedder) = embed::Embedder::try_new(&cache_dir) else {
        return Ok(None);
    };
    let q_vec = match embedder.embed_batch(&[query.to_string()]) {
        Ok(mut v) if !v.is_empty() => v.swap_remove(0),
        _ => return Ok(None),
    };

    let mut ranked: Vec<(String, f32)> = store
        .iter()
        .map(|(id, vec)| (id.to_string(), search::cosine_similarity(&q_vec, vec)))
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(pool_size);
    let candidates: Vec<search::SemanticCandidate> = ranked
        .into_iter()
        .map(|(message_id, similarity)| search::SemanticCandidate {
            message_id,
            similarity,
        })
        .collect();

    let hits = index
        .search_hybrid(query, &candidates, pool_size, filters, hybrid_weight, pool_size)
        .map_err(|e| {
            ErrorEnvelope::new("index-error", format!("hybrid search failed: {e}"))
        })?;
    Ok(Some(hits))
}

#[cfg(feature = "embeddings")]
fn collect_text(message: &Message) -> String {
    use aghist::model::ContentBlock;
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}

/// Best-effort: pull the session's notes from the metadata sidecar, restricted
/// to the exported turn range. Returns `None` if the sidecar can't be opened
/// (the common case: the user hasn't created one yet). Notes that fall outside
/// the slice are dropped; turn-level notes have their `session_ref` rebased so
/// turn N in the full session becomes turn `N - turn_offset` in the slice.
fn load_session_notes(
    session: &Session,
    turn_offset: usize,
    slice_len: usize,
) -> Option<Vec<Note>> {
    let path = metadata::default_path()?;
    if !path.exists() {
        return None;
    }
    let conn = metadata::open(&path).ok()?;
    let session_ref = format!("{}/{}", session.provider.slug(), session.id.0);
    let all = metadata::note_list(&conn, Some(&session_ref)).ok()?;
    let offset = u32::try_from(turn_offset).unwrap_or(u32::MAX);
    let max_turn_inclusive = offset.saturating_add(u32::try_from(slice_len).unwrap_or(u32::MAX));
    let turn_prefix = format!("{session_ref}#");
    let mut out = Vec::with_capacity(all.len());
    for mut n in all {
        if n.session_ref == session_ref {
            out.push(n);
            continue;
        }
        let Some(rest) = n.session_ref.strip_prefix(&turn_prefix) else {
            continue;
        };
        let Ok(turn) = rest.parse::<u32>() else { continue };
        if turn == 0 || turn <= offset || turn > max_turn_inclusive {
            continue;
        }
        let rebased = turn - offset;
        n.session_ref = format!("{turn_prefix}{rebased}");
        out.push(n);
    }
    Some(out)
}

fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
    turn_range: Option<&str>,
    include_notes: bool,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();
    for p in providers {
        if let Ok(sessions) = p.discover_sessions() {
            all_sessions.extend(sessions);
        }
    }

    let session = all_sessions
        .iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!("Session not found: {session_id}"),
            )
            .with_hint("Run `aghist --list` to see available session IDs.")
        })?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == session.provider)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "Provider {} is not enabled for session {}",
                    session.provider, session.id.0
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table).")
        })?;

    let messages = provider.load_messages(session).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let (sliced, turn_offset) = match turn_range {
        Some(spec) => {
            let total = messages.len();
            let (start, end) = parse_turn_range(spec, total).map_err(|msg| {
                ErrorEnvelope::new("usage", msg)
                    .with_hint("Use a 1-based range like `12:25`, `:10`, `5:`, or a single turn `7`.")
            })?;
            // start..end are 1-based inclusive bounds; convert to 0-based half-open.
            // The slice's first message is turn `start` in the original session, so
            // we offset turn-keyed notes by `start - 1` to align them.
            (&messages[(start - 1)..end], start - 1)
        }
        None => (&messages[..], 0usize),
    };

    let notes: Vec<Note> = if include_notes {
        load_session_notes(session, turn_offset, sliced.len()).unwrap_or_default()
    } else {
        Vec::new()
    };

    let content = export::export_with_notes(format, session, sliced, &notes);

    if let Some(path) = output {
        std::fs::write(path, &content).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to write {}: {e}", path.display()),
            )
        })?;
        eprintln!("Exported to {}", path.display());
    } else {
        print!("{content}");
    }

    Ok(EXIT_OK)
}

fn uninstall() -> Result<i32, ErrorEnvelope> {
    let exe = std::env::current_exe()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("current_exe failed: {e}")))?;
    let index_dir = search::SearchIndex::default_index_dir();
    let config_path = config::Config::config_path();
    let config_dir = config_path.as_deref().and_then(|p| p.parent());

    eprintln!("This will remove:");
    eprintln!("  binary:       {}", exe.display());
    if index_dir.exists() {
        eprintln!("  search index: {}", index_dir.display());
    }
    if let Some(dir) = config_dir {
        if dir.exists() {
            eprintln!("  config:       {}", dir.display());
        }
    }

    eprint!("\nContinue? [y/N] ");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to read confirmation: {e}")))?;
    if !input.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        return Err(ErrorEnvelope::new("aborted", "uninstall cancelled by user"));
    }

    if index_dir.exists() {
        std::fs::remove_dir_all(&index_dir).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", index_dir.display()),
            )
        })?;
        eprintln!("Removed {}", index_dir.display());
    }
    if let Some(dir) = config_dir {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to remove {}: {e}", dir.display()),
                )
            })?;
            eprintln!("Removed {}", dir.display());
        }
    }

    // On Windows, self-delete requires renaming first
    #[cfg(windows)]
    {
        let tmp = exe.with_extension("old");
        std::fs::rename(&exe, &tmp).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to rename {} -> {}: {e}", exe.display(), tmp.display()),
            )
        })?;
        if let Err(e) = std::process::Command::new("cmd")
            .args(["/C", "timeout", "/t", "2", "/nobreak", ">nul", "&", "del"])
            .arg(&tmp)
            .spawn()
        {
            eprintln!(
                "warning: could not schedule cleanup of {}: {e}",
                tmp.display()
            );
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(&exe).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", exe.display()),
            )
        })?;
    }

    eprintln!("aghist has been uninstalled.");
    Ok(EXIT_OK)
}

fn self_update() -> Result<i32, ErrorEnvelope> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("Tien-Lam")
        .repo_name("agent-history")
        .bin_name("aghist")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(self_update::cargo_crate_version!())
        .build()
        .map_err(|e| {
            ErrorEnvelope::new("update-failed", format!("failed to configure updater: {e}"))
        })?
        .update()
        .map_err(|e| ErrorEnvelope::new("update-failed", format!("update failed: {e}")))?;

    if status.updated() {
        println!("Updated to v{}", status.version());
    } else {
        println!("Already up to date (v{})", status.version());
    }
    Ok(EXIT_OK)
}

fn resolve_search_query(
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    use std::io::Read;

    let mut sources = 0;
    if query.is_some() {
        sources += 1;
    }
    if query_file.is_some() {
        sources += 1;
    }
    if stdin {
        sources += 1;
    }
    if sources == 0 {
        return Err(ErrorEnvelope::new(
            "usage",
            "search requires a query (positional, --query-file, or --stdin)",
        )
        .with_hint("Run `aghist search --help` for usage."));
    }

    if let Some(q) = query {
        return Ok(q.to_string());
    }

    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
        })?;
    } else if let Some(path) = query_file {
        if path == std::path::Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read query file {}: {e}", path.display()),
                )
            })?;
        }
    }

    Ok(buf.trim_end().to_string())
}

/// Tantivy's `TopDocs::with_limit(N)` materializes only N results, so to
/// paginate by keyset we ask for a generous upper bound, sort with the same
/// tie-break the single-page path uses, then slice past the cursor.
const SEARCH_PAGINATION_POOL: usize = 1000;

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn search_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
    limit: usize,
    cursor: Option<&str>,
    force_json: bool,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    use aghist::model::Session;

    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Ok(EXIT_USAGE);
        }
    };
    let query = resolved.as_str();

    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let after = if let Some(token) = cursor {
        if let Ok(c) = aghist::cursor::SearchCursor::decode(token) {
            Some(c)
        } else {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    } else {
        None
    };

    let federation = federated_discovery_for_search(providers);
    let sessions: Vec<Session> = federation.sessions;

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    // Incremental index update — fast on subsequent calls (manifest tracks mtimes).
    // We don't surface progress for the CLI, so drain into a sender we discard.
    // The local provider list is sufficient: build_index loads messages from
    // each session's absolute `source_path`, which already points at the
    // remote cache for federated sessions.
    let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
    index.build_index(&sessions, providers, &tx).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
    })?;

    // Best-effort: index user notes so they show up alongside messages.
    // Sidecar absence (no metadata.db on disk yet) is the common case and must
    // not fail search — we just skip silently.
    try_index_notes(&index);

    // Always pull the pagination pool so cursor resumption sees a stable
    // ordering across calls. Tantivy ranks by score, but we re-sort below
    // with our deterministic tie-break.
    let pool_size = if cursor.is_some() {
        SEARCH_PAGINATION_POOL
    } else {
        limit.max(1)
    };

    // Hybrid path: only attempt when the user opted in. Falls open to lexical
    // if the embedding pipeline isn't ready (no consent / empty store / lean
    // build / embedder fails) — see `try_hybrid_search`.
    let mut engine_used = "lexical";
    let hybrid_hits: Option<Vec<search::SearchHit>> = if hybrid_weight > 0.0 {
        try_hybrid_search(
            &index_dir,
            &index,
            query,
            pool_size,
            filters,
            hybrid_weight,
        )?
    } else {
        None
    };

    let raw_hits: Vec<(search::SearchHit, Option<search::Explanation>)> = if let Some(hits) =
        hybrid_hits
    {
        engine_used = "hybrid";
        // Hybrid scores are RRF-fused — Tantivy explanations describe the BM25
        // contribution only and would be misleading attached to a fused score,
        // so we omit them. `--debug-search` is documented as lexical-only.
        hits.into_iter().map(|h| (h, None)).collect()
    } else if debug_search {
        index
            .search_with_filters_and_explain(query, pool_size, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
            .into_iter()
            .map(|(h, e)| (h, Some(e)))
            .collect()
    } else {
        index
            .search_with_filters(query, pool_size, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
            .into_iter()
            .map(|h| (h, None))
            .collect()
    };

    // Tie-break by (started_at DESC, session_id ASC) for deterministic ordering.
    // Tantivy already returns score-DESC; we use a stable sort to preserve that
    // and only reorder ties.
    let session_meta: std::collections::HashMap<&str, &Session> =
        sessions.iter().map(|s| (s.id.0.as_str(), s)).collect();

    // Metadata filter post-filters hits whose session isn't in the allowed
    // `<provider>/<id>` set. Index-level filtering would couple the search
    // crate to the metadata sidecar; post-filter keeps separation of concerns
    // and avoids reindexing whenever a tag/star/note is added or removed.
    let raw_hits: Vec<(search::SearchHit, Option<search::Explanation>)> =
        if let Some(keys) = metadata_keys {
            raw_hits
                .into_iter()
                .filter(|(hit, _)| match hit.kind {
                    search::HitKind::Message => session_meta
                        .get(hit.session_id.as_str())
                        .map(|s| session_metadata_key(s))
                        .is_some_and(|k| keys.contains(&k)),
                    // For note hits the canonical key is the note's target
                    // (`<provider>/<session-id>`, turn suffix stripped) — keep
                    // the note iff its target session is in the allowed set,
                    // mirroring how notes are surfaced as session annotations.
                    search::HitKind::Note => hit
                        .note_session_ref
                        .as_deref()
                        .map(strip_turn_suffix)
                        .is_some_and(|k| keys.contains(k)),
                })
                .collect()
        } else {
            raw_hits
        };

    let total = raw_hits.len();

    if raw_hits.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut ordered = raw_hits;
    ordered.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_started = session_meta
                    .get(a.0.session_id.as_str())
                    .map(|s| s.started_at);
                let b_started = session_meta
                    .get(b.0.session_id.as_str())
                    .map(|s| s.started_at);
                b_started.cmp(&a_started)
            })
            .then_with(|| a.0.session_id.cmp(&b.0.session_id))
    });

    let page_start = match &after {
        Some(c) => ordered
            .iter()
            .position(|(h, _)| {
                // Strictly past the cursor in score-DESC, id-ASC order.
                // Exact f32 equality is intentional — both sides come from
                // Tantivy's deterministic scoring for the same query, not
                // arithmetic that would introduce floating-point drift.
                #[allow(clippy::float_cmp)]
                {
                    h.score < c.score || (h.score == c.score && h.session_id > c.session_id)
                }
            })
            .unwrap_or(ordered.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(ordered.len());
    let page = &ordered[page_start..page_end];

    if page.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let next_cursor = if page_end < ordered.len() {
        page.last().map(|(h, _)| {
            aghist::cursor::SearchCursor {
                score: h.score,
                session_id: h.session_id.clone(),
            }
            .encode()
        })
    } else {
        None
    };

    let want_json = force_json || !io::stdout().is_terminal();

    if want_json {
        print_search_json(
            page,
            &session_meta,
            &federation.source_by_session,
            total,
            next_cursor.as_deref(),
            engine_used,
        )
        .map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_search_table(
            page,
            &session_meta,
            &federation.source_by_session,
            next_cursor.as_deref(),
        );
    }

    Ok(EXIT_OK)
}

/// Discover sessions from local providers + every registered remote source.
///
/// Falls back to local-only when the config or sources cache root cannot be
/// resolved (e.g. headless test envs without a HOME dir). Source-level
/// failures are emitted as `warning:` lines on stderr but never fatal — search
/// remains usable as long as at least one source returned sessions.
fn federated_discovery_for_search(
    providers: &[Box<dyn provider::HistoryProvider>],
) -> federated::FederatedDiscovery {
    let sources = match config::Config::resolved_path() {
        Some(path) => config::Config::load_from(&path).sources,
        None => Vec::new(),
    };
    let Some(cache_root) = config::sources_cache_root() else {
        // No cache root means no remote sources can be consulted.
        // Discover locally only — same path as before y3o.6.3.
        return federated::discover_federated(providers, &[], std::path::Path::new(""));
    };
    let result = federated::discover_federated(providers, &sources, &cache_root);
    for failure in &result.failures {
        eprintln!(
            "warning: source '{}': {}",
            failure.source, failure.message
        );
    }
    result
}

fn print_search_json(
    hits: &[(search::SearchHit, Option<search::Explanation>)],
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
    source_by_session: &std::collections::HashMap<String, String>,
    total: usize,
    next_cursor: Option<&str>,
    engine: &str,
) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonHit<'a> {
        /// Stable discriminator: `"message"` (default) or `"note"`. Agents can
        /// branch on this without inspecting which optional fields are present.
        kind: &'static str,
        session_id: &'a str,
        message_id: &'a str,
        score: f32,
        snippet: &'a str,
        provider: Option<aghist::model::Provider>,
        project: Option<&'a str>,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
        /// Origin of the session: `"local"` for the host's own provider dirs,
        /// or a registered remote source name (see `aghist sources list`).
        source: &'a str,
        /// Populated for `kind="note"`: the metadata.db row id.
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<i64>,
        /// Populated for `kind="note"`: citation-style ref for the note's
        /// target (`<provider>/<session-id>[#<turn>]`).
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        ref_: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        explanation: Option<&'a search::Explanation>,
    }

    let rows: Vec<JsonHit> = hits
        .iter()
        .map(|(h, explain)| match h.kind {
            search::HitKind::Note => JsonHit {
                kind: search::HitKind::Note.slug(),
                session_id: &h.session_id,
                message_id: &h.message_id,
                score: h.score,
                snippet: &h.snippet,
                provider: None,
                project: None,
                started_at: None,
                source: federated::LOCAL_SOURCE,
                note_id: h.note_id,
                ref_: h.note_session_ref.as_deref(),
                explanation: explain.as_ref(),
            },
            search::HitKind::Message => {
                let session = sessions.get(h.session_id.as_str()).copied();
                let source = source_by_session
                    .get(h.session_id.as_str())
                    .map_or(federated::LOCAL_SOURCE, String::as_str);
                JsonHit {
                    kind: search::HitKind::Message.slug(),
                    session_id: &h.session_id,
                    message_id: &h.message_id,
                    score: h.score,
                    snippet: &h.snippet,
                    provider: session.map(|s| s.provider),
                    project: session.and_then(|s| s.project_name.as_deref()),
                    started_at: session.map(|s| s.started_at),
                    source,
                    note_id: None,
                    ref_: None,
                    explanation: explain.as_ref(),
                }
            }
        })
        .collect();

    let doc = serde_json::json!({
        "hits": rows,
        "meta": { "next_cursor": next_cursor, "total": total, "engine": engine },
    });
    serde_json::to_writer(io::stdout().lock(), &doc)?;
    println!();
    Ok(())
}

fn print_search_table(
    hits: &[(search::SearchHit, Option<search::Explanation>)],
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
    source_by_session: &std::collections::HashMap<String, String>,
    next_cursor: Option<&str>,
) {
    // Only show the SOURCE column when at least one hit is non-local — keeps
    // the local-only output identical to the pre-federated layout so existing
    // users and snapshot-style reads aren't disrupted.
    let any_remote = hits.iter().any(|(h, _)| {
        source_by_session
            .get(h.session_id.as_str())
            .is_some_and(|s| s != federated::LOCAL_SOURCE)
    });

    if any_remote {
        println!(
            "{:<6}  {:<16}  {:<12}  {:<20}  {:<10}  {:<14}  SNIPPET",
            "SCORE", "STARTED", "PROVIDER", "PROJECT", "SOURCE", "SESSION"
        );
    } else {
        println!(
            "{:<6}  {:<16}  {:<12}  {:<20}  {:<14}  SNIPPET",
            "SCORE", "STARTED", "PROVIDER", "PROJECT", "SESSION"
        );
    }
    for (h, explain) in hits {
        let is_note = matches!(h.kind, search::HitKind::Note);
        let session = sessions.get(h.session_id.as_str()).copied();
        let started = if is_note {
            String::new()
        } else {
            session
                .map(|s| s.started_at.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default()
        };
        // For note rows the PROVIDER column carries the `note` discriminator
        // so the row is visually distinct without needing a new column. The
        // PROJECT column shows the note's session_ref (turn-stripped) so the
        // human reader can still locate the underlying session.
        let provider = if is_note {
            "note"
        } else {
            session.map_or("", |s| s.provider.as_str())
        };
        let project_owned = if is_note {
            h.note_session_ref
                .as_deref()
                .map(|r| strip_turn_suffix(r).to_string())
                .unwrap_or_default()
        } else {
            session
                .and_then(|s| s.project_name.as_deref())
                .unwrap_or("")
                .to_string()
        };
        let project = truncate(&project_owned, 20);
        let session_label = if is_note {
            h.note_id.map_or_else(String::new, |id| format!("note#{id}"))
        } else {
            h.session_id.clone()
        };
        let session_short = truncate(&session_label, 14);
        let snippet = truncate(&h.snippet, 80);
        if any_remote {
            let source = source_by_session
                .get(h.session_id.as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str);
            let source = truncate(source, 10);
            println!(
                "{:<6.2}  {:<16}  {:<12}  {:<20}  {:<10}  {:<14}  {}",
                h.score, started, provider, project, source, session_short, snippet
            );
        } else {
            println!(
                "{:<6.2}  {:<16}  {:<12}  {:<20}  {:<14}  {}",
                h.score, started, provider, project, session_short, snippet
            );
        }
        if let Some(explanation) = explain {
            for line in explanation.to_pretty_json().lines() {
                println!("    {line}");
            }
        }
    }
    if let Some(token) = next_cursor {
        println!("\n(more results — pass --cursor {token} for the next page)");
    }
}

/// Long-running NDJSON stream: poll for newly-indexed sessions and emit
/// previously-unseen hits matching `query`.
///
/// First iteration backfills all current matches (so a fresh subscriber sees
/// existing state); subsequent iterations only emit `(session_id, message_id)`
/// pairs that have not been emitted before. Exits cleanly on broken pipe so
/// `aghist search ... --watch | head -N` works.
#[allow(clippy::too_many_arguments)]
fn search_watch_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
    limit: usize,
    interval_ms: u64,
    max_iterations: u32,
    filters: &SearchFilters,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    use aghist::model::Session;
    use std::collections::HashSet;
    use std::io::Write;

    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Ok(EXIT_USAGE);
        }
    };
    let query = resolved.as_str();
    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    let interval = std::time::Duration::from_millis(interval_ms);
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut iteration: u32 = 0;
    let stdout = io::stdout();

    loop {
        iteration += 1;

        let federation = federated_discovery_for_search(providers);
        let sessions: Vec<Session> = federation.sessions;

        let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
        index.build_index(&sessions, providers, &tx).map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
        })?;

        try_index_notes(&index);

        let hits = index
            .search_with_filters(query, limit, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?;

        let session_meta: std::collections::HashMap<&str, &Session> =
            sessions.iter().map(|s| (s.id.0.as_str(), s)).collect();

        let mut handle = stdout.lock();
        for h in &hits {
            if let Some(keys) = metadata_keys {
                let allowed = session_meta
                    .get(h.session_id.as_str())
                    .map(|s| session_metadata_key(s))
                    .is_some_and(|k| keys.contains(&k));
                if !allowed {
                    continue;
                }
            }
            let key = (h.session_id.clone(), h.message_id.clone());
            if !seen.insert(key) {
                continue;
            }
            if write_watch_hit(&mut handle, h, &session_meta, &federation.source_by_session)
                .is_err()
            {
                // Broken pipe (downstream closed) — exit cleanly.
                return Ok(EXIT_OK);
            }
        }
        // Flush so consumers see lines promptly between sleeps.
        if handle.flush().is_err() {
            return Ok(EXIT_OK);
        }
        drop(handle);

        if max_iterations > 0 && iteration >= max_iterations {
            return Ok(EXIT_OK);
        }

        std::thread::sleep(interval);
    }
}

fn write_watch_hit<W: std::io::Write>(
    out: &mut W,
    hit: &search::SearchHit,
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
    source_by_session: &std::collections::HashMap<String, String>,
) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonHit<'a> {
        kind: &'static str,
        session_id: &'a str,
        message_id: &'a str,
        score: f32,
        snippet: &'a str,
        provider: Option<aghist::model::Provider>,
        project: Option<&'a str>,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
        source: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<i64>,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        ref_: Option<&'a str>,
    }

    let row = match hit.kind {
        search::HitKind::Note => JsonHit {
            kind: search::HitKind::Note.slug(),
            session_id: &hit.session_id,
            message_id: &hit.message_id,
            score: hit.score,
            snippet: &hit.snippet,
            provider: None,
            project: None,
            started_at: None,
            source: federated::LOCAL_SOURCE,
            note_id: hit.note_id,
            ref_: hit.note_session_ref.as_deref(),
        },
        search::HitKind::Message => {
            let session = sessions.get(hit.session_id.as_str()).copied();
            let source = source_by_session
                .get(hit.session_id.as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str);
            JsonHit {
                kind: search::HitKind::Message.slug(),
                session_id: &hit.session_id,
                message_id: &hit.message_id,
                score: hit.score,
                snippet: &hit.snippet,
                provider: session.map(|s| s.provider),
                project: session.and_then(|s| s.project_name.as_deref()),
                started_at: session.map(|s| s.started_at),
                source,
                note_id: None,
                ref_: None,
            }
        }
    };
    serde_json::to_writer(&mut *out, &row)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
    limit: usize,
    cursor: Option<&str>,
    filters: &FilterArgs,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();

    let needs_messages = filters.role.is_some() || filters.has_tool_call;
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    for p in providers {
        // When --provider is set, skip non-matching providers entirely so we
        // don't pay discovery cost for sessions we'd just throw away.
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(sessions) => {
                let kept: Vec<Session> = sessions
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref()))
                    .filter(|s| metadata_filter_matches(s, metadata_keys))
                    .filter(|s| {
                        !needs_messages || session_has_matching_message(p.as_ref(), s, filters)
                    })
                    .collect();
                if !mode.is_machine() {
                    println!("{}: {} sessions", p.provider(), kept.len());
                }
                all_sessions.extend(kept);
            }
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
            }
        }
    }

    // Canonical sort: started_at DESC, session_id ASC. The id tie-break makes
    // the cursor's keyset comparison total even when two sessions share a
    // millisecond timestamp.
    all_sessions.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    let total = all_sessions.len();

    let after = if let Some(token) = cursor {
        if let Ok(c) = aghist::cursor::ListCursor::decode(token) {
            Some(c)
        } else {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    } else {
        None
    };

    let page_start = match &after {
        Some(c) => all_sessions
            .iter()
            .position(|s| {
                // Match the canonical order: started_at DESC, id ASC. We want
                // the first session strictly *after* the cursor key.
                s.started_at < c.started_at
                    || (s.started_at == c.started_at && s.id.0 > c.session_id)
            })
            .unwrap_or(all_sessions.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(all_sessions.len());
    let page = &all_sessions[page_start..page_end];

    let next_cursor = if page_end < all_sessions.len() {
        page.last().map(|s| {
            aghist::cursor::ListCursor {
                started_at: s.started_at,
                session_id: s.id.0.clone(),
            }
            .encode()
        })
    } else {
        None
    };

    match mode {
        OutputMode::Human => render_list_human(page, total, next_cursor.as_deref()),
        OutputMode::Json => render_list_json(page, total, next_cursor.as_deref()).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?,
        OutputMode::Ndjson => {
            render_list_ndjson(page, total, next_cursor.as_deref()).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to write NDJSON output: {e}"))
            })?;
        }
    }

    if all_sessions.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

/// Apply session-level filters (provider, since/until, project). Provider is
/// not re-checked here when the caller already filtered by provider, but it's
/// harmless to do so. `project_needle` is the pre-lowercased substring for
/// efficiency in the per-session loop.
fn session_matches(
    session: &Session,
    filters: &FilterArgs,
    project_needle: Option<&str>,
) -> bool {
    if let Some(want) = filters.provider {
        if session.provider != want {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if session.started_at < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if session.started_at > until {
            return false;
        }
    }
    if let Some(needle) = project_needle {
        let project = session
            .project_name
            .as_deref()
            .map(str::to_lowercase)
            .unwrap_or_default();
        if !project.contains(needle) {
            return false;
        }
    }
    true
}

/// Resolve `--note`/`--tag`/`--starred` into a set of
/// `<provider-slug>/<session-id>` keys, opening the metadata sidecar on
/// demand. Returns `Ok(None)` when no metadata filter is requested so the
/// caller can skip the lookup entirely (and avoid creating the DB on disk).
fn resolve_metadata_filter(
    filters: &FilterArgs,
) -> Result<Option<std::collections::HashSet<String>>, ErrorEnvelope> {
    if !filters.has_metadata_filter() {
        return Ok(None);
    }
    let conn = open_metadata_db()?;
    metadata::filter_session_keys(
        &conn,
        filters.note.as_deref(),
        filters.tag.as_deref(),
        filters.starred,
    )
    .map_err(|e| metadata_error(&e))
}

/// Build the canonical metadata key for a session: `<provider-slug>/<id>`.
fn session_metadata_key(session: &Session) -> String {
    format!("{}/{}", session.provider.slug(), session.id.0)
}

/// Drop a `#<turn>` suffix, leaving `<provider-slug>/<session-id>` — the same
/// key shape as [`session_metadata_key`] so note refs and session refs can be
/// compared against the same allow-set.
fn strip_turn_suffix(session_ref: &str) -> &str {
    session_ref.rsplit_once('#').map_or(session_ref, |(prefix, _)| prefix)
}

/// Returns true when `metadata_keys` is `None` (filter inactive) or when the
/// session's `<provider>/<id>` key is in the allowed set.
fn metadata_filter_matches(
    session: &Session,
    metadata_keys: Option<&std::collections::HashSet<String>>,
) -> bool {
    let Some(keys) = metadata_keys else {
        return true;
    };
    keys.contains(&session_metadata_key(session))
}

/// Returns true if the session contains at least one message satisfying the
/// message-level filters (`--role`, `--has-tool-call`). Loads messages on
/// demand; corrupt/unreadable sessions are silently dropped (consistent with
/// the rest of the pipeline).
fn session_has_matching_message(
    provider: &dyn provider::HistoryProvider,
    session: &Session,
    filters: &FilterArgs,
) -> bool {
    let Ok(messages) = provider.load_messages(session) else {
        return false;
    };
    messages.iter().any(|m| message_matches(m, filters))
}

fn message_matches(message: &Message, filters: &FilterArgs) -> bool {
    if let Some(role) = filters.role {
        if message.role != role {
            return false;
        }
    }
    if filters.has_tool_call
        && !message
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse(_)))
    {
        return false;
    }
    true
}

fn render_list_human(sessions: &[Session], total: usize, next_cursor: Option<&str>) {
    println!("\nTotal: {total} sessions\n");
    for s in sessions {
        let project = s.project_name.as_deref().unwrap_or("(unknown)");
        let branch = s.git_branch.as_deref().unwrap_or("");
        let summary = match s.summary.as_deref() {
            Some(text) if text.chars().count() > 60 => {
                let mut s: String = text.chars().take(57).collect();
                s.push_str("...");
                s
            }
            Some(text) => text.to_string(),
            None => String::new(),
        };
        println!(
            "  {} | {} | {} | {} | {}",
            s.started_at.format("%Y-%m-%d %H:%M"),
            s.provider,
            project,
            branch,
            summary
        );
    }
    if let Some(token) = next_cursor {
        println!("\n(more results — pass --cursor {token} for the next page)");
    }
}

#[derive(serde::Serialize)]
struct SessionRow<'a> {
    id: &'a str,
    provider: aghist::model::Provider,
    project: Option<&'a str>,
    branch: Option<&'a str>,
    summary: Option<&'a str>,
    started_at: chrono::DateTime<chrono::Utc>,
    message_count: usize,
}

impl<'a> SessionRow<'a> {
    fn from_session(s: &'a Session) -> Self {
        Self {
            id: s.id.0.as_str(),
            provider: s.provider,
            project: s.project_name.as_deref(),
            branch: s.git_branch.as_deref(),
            summary: s.summary.as_deref(),
            started_at: s.started_at,
            message_count: s.message_count,
        }
    }
}

fn render_list_json(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let rows: Vec<SessionRow<'_>> = sessions.iter().map(SessionRow::from_session).collect();
    let doc = serde_json::json!({
        "sessions": rows,
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &doc).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_list_ndjson(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = SessionRow::from_session(s);
        serde_json::to_writer(&mut out, &row).map_err(std::io::Error::other)?;
        writeln!(out)?;
    }
    // Trailing meta record terminates the stream so consumers can detect EOF
    // without watching stdin close. Keyed by `meta` so it never collides with
    // a session row (which is keyed by `id`).
    let meta = serde_json::json!({
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    serde_json::to_writer(&mut out, &meta).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn health_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let checks = health::run_health_checks(providers);
    let fidelity = health::run_provider_fidelity(providers);
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_health_human(&mut out, &checks, &fidelity),
        OutputMode::Json | OutputMode::Ndjson => {
            render_health_json(&mut out, &checks, &fidelity, !any_failed)
        }
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write health output: {e}")))?;

    Ok(if any_failed { EXIT_ERROR } else { EXIT_OK })
}

fn render_health_human<W: io::Write>(
    out: &mut W,
    checks: &[HealthCheck],
    fidelity: &[aghist::provider_diagnostic::ProviderDiagnostic],
) -> io::Result<()> {
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);
    let any_warn = checks.iter().any(|c| c.status == HealthStatus::Warn);
    let summary = if any_failed {
        "FAIL"
    } else if any_warn {
        "WARN"
    } else {
        "OK"
    };
    writeln!(out, "Overall: {summary}")?;
    writeln!(out)?;
    for c in checks {
        let tag = match c.status {
            HealthStatus::Ok => "OK  ",
            HealthStatus::Warn => "WARN",
            HealthStatus::Fail => "FAIL",
        };
        writeln!(out, "  [{tag}] {} — {}", c.name, c.message)?;
        if let Some(hint) = &c.hint {
            writeln!(out, "         hint: {hint}")?;
        }
    }
    if !fidelity.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "Provider fidelity (sample of up to {} sessions per provider):",
            health::HEALTH_FIDELITY_SAMPLE_PER_PROVIDER,
        )?;
        for d in fidelity {
            let f = &d.tool_call_fidelity;
            writeln!(
                out,
                "  {} ({}): sessions={} messages={} tool_calls={} paired={} unpaired={} orphan_results={} empty_names={}",
                d.label,
                d.provider,
                d.session_count,
                d.message_count,
                f.tool_calls,
                f.paired,
                f.unpaired_calls,
                f.orphan_results,
                f.empty_names,
            )?;
        }
    }
    Ok(())
}

fn render_health_json<W: io::Write>(
    out: &mut W,
    checks: &[HealthCheck],
    fidelity: &[aghist::provider_diagnostic::ProviderDiagnostic],
    ok: bool,
) -> io::Result<()> {
    let summary = serde_json::json!({
        "ok_count": checks.iter().filter(|c| c.status == HealthStatus::Ok).count(),
        "warn_count": checks.iter().filter(|c| c.status == HealthStatus::Warn).count(),
        "fail_count": checks.iter().filter(|c| c.status == HealthStatus::Fail).count(),
    });
    let payload = serde_json::json!({
        "ok": ok,
        "checks": checks,
        "summary": summary,
        "provider_fidelity": fidelity,
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn note_dispatch(command: NoteCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        NoteCommand::Add {
            reference,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note = metadata::note_add(&conn, &reference, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "added", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::List { reference, json } => {
            let mode = if json { OutputMode::Json } else { mode };
            let notes = metadata::note_list(&conn, reference.as_deref()).map_err(|e| metadata_error(&e))?;
            emit_note_list(&notes, mode)?;
            if notes.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        NoteCommand::Edit {
            id,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note = metadata::note_edit(&conn, id, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "updated", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::Remove { id } => {
            let note = metadata::note_remove(&conn, id).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn open_metadata_db() -> Result<rusqlite::Connection, ErrorEnvelope> {
    metadata::open_default().map_err(|e| metadata_error(&e))
}

/// Best-effort: open the metadata sidecar and feed every note into the search
/// index. Any failure (sidecar not yet created, IO error, malformed row) is
/// swallowed — `aghist search` must keep working without notes when the
/// sidecar is unavailable, since metadata is opt-in.
fn try_index_notes(index: &search::SearchIndex) {
    let Some(path) = metadata::default_path() else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(conn) = metadata::open(&path) else {
        return;
    };
    let Ok(notes) = metadata::note_list(&conn, None) else {
        return;
    };
    let _ = index.index_notes(&notes);
}

fn metadata_error(err: &MetadataError) -> ErrorEnvelope {
    match err {
        MetadataError::NoPath => ErrorEnvelope::new(
            "config-error",
            "could not resolve metadata.db path",
        )
        .with_hint("Set AGHIST_METADATA_DB=/path/to/metadata.db, or ensure XDG/home dirs exist."),
        MetadataError::CreateDir { ref path, .. } => ErrorEnvelope::new(
            "io-error",
            format!("could not create metadata dir {}: {err}", path.display()),
        ),
        MetadataError::Open { ref path, .. } => ErrorEnvelope::new(
            "io-error",
            format!("could not open metadata.db at {}: {err}", path.display()),
        ),
        MetadataError::Migrate { ref path, .. } => ErrorEnvelope::new(
            "metadata-error",
            format!("metadata.db migration failed at {}: {err}", path.display()),
        ),
        MetadataError::InvalidSessionRef(_, _) => ErrorEnvelope::new(
            "invalid-ref",
            err.to_string(),
        )
        .with_hint(
            "Use '<provider>/<session-id>' or '<provider>/<session-id>#<turn>'. \
             Valid providers: claude-code, copilot-cli, gemini-cli, codex-cli, opencode.",
        ),
        MetadataError::EmptyBody => ErrorEnvelope::new(
            "usage",
            "note body must not be empty",
        )
        .with_hint("Pass --body \"text\", --body-file PATH, or --stdin."),
        MetadataError::NoteNotFound(id) => ErrorEnvelope::new(
            "note-not-found",
            format!("no note with id {id}"),
        )
        .with_hint("Run `aghist note list` to see existing note ids."),
        MetadataError::EmptyTag => ErrorEnvelope::new("usage", "tag must not be empty")
            .with_hint("Pass a non-empty tag value, e.g. `aghist tag add <ref> review`."),
        MetadataError::TagAlreadyExists { session_ref, tag } => ErrorEnvelope::new(
            "tag-conflict",
            format!("tag '{tag}' is already attached to {session_ref}"),
        )
        .with_hint("Each (session_ref, tag) pair is unique. Use a different tag, or remove the existing one first."),
        MetadataError::TagNotFound { session_ref, tag } => ErrorEnvelope::new(
            "tag-not-found",
            format!("tag '{tag}' is not attached to {session_ref}"),
        )
        .with_hint("Run `aghist tag list <ref>` to see attached tags."),
        MetadataError::StarAlreadyExists { session_ref } => ErrorEnvelope::new(
            "star-conflict",
            format!("{session_ref} is already starred"),
        )
        .with_hint("Each session ref can be starred at most once. Use `aghist unstar <ref>` first if you want to re-star."),
        MetadataError::StarNotFound { session_ref } => ErrorEnvelope::new(
            "star-not-found",
            format!("{session_ref} is not starred"),
        )
        .with_hint("Run `aghist stars` to see starred refs."),
        MetadataError::Sqlite(_) => ErrorEnvelope::new("metadata-error", err.to_string()),
    }
}

fn read_note_body(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    use std::io::Read;
    if let Some(b) = body {
        return Ok(b.to_string());
    }
    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to read note body from stdin: {e}"))
        })?;
        return Ok(buf);
    }
    if let Some(path) = body_file {
        if path == std::path::Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to read note body from stdin: {e}"))
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read note body from {}: {e}", path.display()),
                )
            })?;
        }
        return Ok(buf);
    }
    Err(ErrorEnvelope::new(
        "usage",
        "note body required: pass --body, --body-file, or --stdin",
    ))
}

fn emit_note_payload(note: &Note, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: note });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} note {} on {}", note.id, note.session_ref).ok();
        for line in note.body.lines() {
            writeln!(out, "  {line}").ok();
        }
    }
    Ok(())
}

fn emit_note_list(notes: &[Note], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "notes": notes, "count": notes.len() });
            serde_json::to_writer(&mut out, &payload).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}"))
            })?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for note in notes {
                serde_json::to_writer(&mut out, note).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if notes.is_empty() {
                writeln!(out, "(no notes)").ok();
            } else {
                for note in notes {
                    writeln!(
                        out,
                        "#{} {} (created {}, updated {})",
                        note.id, note.session_ref, note.created_at, note.updated_at
                    )
                    .ok();
                    for line in note.body.lines() {
                        writeln!(out, "  {line}").ok();
                    }
                }
            }
        }
    }
    Ok(())
}

fn tag_dispatch(command: TagCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        TagCommand::Add { reference, tag } => {
            let row = metadata::tag_add(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "added", mode)?;
            Ok(EXIT_OK)
        }
        TagCommand::List {
            reference,
            tag,
            json,
        } => {
            let mode = if json { OutputMode::Json } else { mode };
            let tags = metadata::tag_list(&conn, reference.as_deref(), tag.as_deref())
                .map_err(|e| metadata_error(&e))?;
            emit_tag_list(&tags, mode)?;
            if tags.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        TagCommand::Remove { reference, tag } => {
            let row =
                metadata::tag_remove(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn emit_tag_payload(tag: &Tag, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: tag });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} tag '{}' on {}", tag.tag, tag.session_ref).ok();
    }
    Ok(())
}

fn emit_tag_list(tags: &[Tag], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "tags": tags, "count": tags.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for tag in tags {
                serde_json::to_writer(&mut out, tag).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if tags.is_empty() {
                writeln!(out, "(no tags)").ok();
            } else {
                for tag in tags {
                    writeln!(
                        out,
                        "#{} {} [{}] (created {})",
                        tag.id, tag.session_ref, tag.tag, tag.created_at
                    )
                    .ok();
                }
            }
        }
    }
    Ok(())
}

fn star_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_add(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "starred", mode)?;
    Ok(EXIT_OK)
}

fn unstar_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_remove(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "unstarred", mode)?;
    Ok(EXIT_OK)
}

fn stars_list(reference: Option<&str>, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let stars = metadata::star_list(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_list(&stars, mode)?;
    if stars.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn emit_star_payload(star: &Star, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: star });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} {}", star.session_ref).ok();
    }
    Ok(())
}

fn emit_star_list(stars: &[Star], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "stars": stars, "count": stars.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for star in stars {
                serde_json::to_writer(&mut out, star).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if stars.is_empty() {
                writeln!(out, "(no stars)").ok();
            } else {
                for star in stars {
                    writeln!(out, "★ {} (starred {})", star.session_ref, star.starred_at).ok();
                }
            }
        }
    }
    Ok(())
}

fn sources_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let index_dir = search::SearchIndex::default_index_dir();
    let manifest_path = index_dir.join("manifest.json");
    let last_indexed_at = std::fs::metadata(&manifest_path)
        .and_then(|m| m.modified())
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from);

    let rows: Vec<SourceRow> = providers
        .iter()
        .map(|p| collect_source_row(p.as_ref(), &index_dir, last_indexed_at))
        .collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_sources_human(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Json => render_sources_json(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Ndjson => render_sources_ndjson(&mut out, &rows),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write sources output: {e}")))?;

    if rows.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

#[derive(serde::Serialize)]
struct SourceRow {
    provider: Provider,
    paths: Vec<SourcePath>,
    session_count: usize,
    total_bytes: u64,
    discover_error: Option<String>,
}

#[derive(serde::Serialize)]
struct SourcePath {
    path: String,
    exists: bool,
    bytes: u64,
}

fn collect_source_row(
    p: &dyn provider::HistoryProvider,
    _index_dir: &std::path::Path,
    _last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> SourceRow {
    let mut paths = Vec::new();
    let mut total_bytes: u64 = 0;
    for dir in p.base_dirs() {
        let exists = dir.exists();
        let bytes = if exists { dir_size_bytes(dir) } else { 0 };
        total_bytes = total_bytes.saturating_add(bytes);
        paths.push(SourcePath {
            path: dir.display().to_string(),
            exists,
            bytes,
        });
    }

    let (session_count, discover_error) = match p.discover_sessions() {
        Ok(s) => (s.len(), None),
        Err(e) => (0, Some(e.to_string())),
    };

    SourceRow {
        provider: p.provider(),
        paths,
        session_count,
        total_bytes,
        discover_error,
    }
}

/// Recursive directory size in bytes. Symlinks and IO errors are skipped.
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        } else if meta.is_dir() {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        }
    }
    total
}

fn render_sources_human<W: io::Write>(
    out: &mut W,
    rows: &[SourceRow],
    index_dir: &std::path::Path,
    last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> io::Result<()> {
    if rows.is_empty() {
        writeln!(out, "No providers detected. Check your config (`providers` table).")?;
        return Ok(());
    }
    writeln!(
        out,
        "{:<14}  {:<8}  {:<10}  PATHS",
        "PROVIDER", "SESSIONS", "SIZE"
    )?;
    for row in rows {
        let paths_str = row
            .paths
            .iter()
            .map(|p| {
                if p.exists {
                    p.path.clone()
                } else {
                    format!("{} (missing)", p.path)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let size = format_bytes(row.total_bytes);
        writeln!(
            out,
            "{:<14}  {:<8}  {:<10}  {paths_str}",
            row.provider.slug(),
            row.session_count,
            size
        )?;
        if let Some(err) = &row.discover_error {
            writeln!(out, "  ! discover error: {err}")?;
        }
    }
    writeln!(out)?;
    writeln!(out, "Index dir: {}", index_dir.display())?;
    if let Some(ts) = last_indexed_at {
        writeln!(out, "Last indexed: {}", ts.format("%Y-%m-%d %H:%M UTC"))?;
    } else {
        writeln!(out, "Last indexed: never (run `aghist index`)")?;
    }
    Ok(())
}

fn render_sources_json<W: io::Write>(
    out: &mut W,
    rows: &[SourceRow],
    index_dir: &std::path::Path,
    last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> io::Result<()> {
    let payload = serde_json::json!({
        "sources": rows,
        "index": {
            "dir": index_dir.display().to_string(),
            "last_indexed_at": last_indexed_at,
        },
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_sources_ndjson<W: io::Write>(out: &mut W, rows: &[SourceRow]) -> io::Result<()> {
    for row in rows {
        serde_json::to_writer(&mut *out, row).map_err(std::io::Error::other)?;
        writeln!(out)?;
    }
    Ok(())
}

fn resolve_config_path() -> Result<PathBuf, ErrorEnvelope> {
    config::Config::resolved_path().ok_or_else(|| {
        ErrorEnvelope::new(
            "config-error",
            "could not determine config path; HOME and XDG_CONFIG_HOME are unset",
        )
        .with_hint("Set AGHIST_CONFIG=/path/to/config.toml to override.")
    })
}

fn write_sources_payload<W: io::Write>(
    out: &mut W,
    sources: &[config::RemoteSource],
    config_path: &std::path::Path,
    mode: OutputMode,
) -> io::Result<()> {
    match mode {
        OutputMode::Human => render_remote_sources_human(out, sources, config_path),
        OutputMode::Json => {
            let payload = serde_json::json!({
                "sources": sources,
                "config_path": config_path.display().to_string(),
            });
            serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
            writeln!(out)
        }
        OutputMode::Ndjson => {
            for s in sources {
                serde_json::to_writer(&mut *out, s).map_err(std::io::Error::other)?;
                writeln!(out)?;
            }
            Ok(())
        }
    }
}

fn render_remote_sources_human<W: io::Write>(
    out: &mut W,
    sources: &[config::RemoteSource],
    config_path: &std::path::Path,
) -> io::Result<()> {
    if sources.is_empty() {
        writeln!(
            out,
            "No remote sources registered. Add one with `aghist sources add <name> --host <host> --path <path>`."
        )?;
        writeln!(out, "Config: {}", config_path.display())?;
        return Ok(());
    }
    writeln!(
        out,
        "{:<20}  {:<10}  {:<25}  PATH",
        "NAME", "TRANSPORT", "HOST"
    )?;
    for s in sources {
        writeln!(
            out,
            "{:<20}  {:<10}  {:<25}  {}",
            s.name,
            s.transport.slug(),
            s.host,
            s.path
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Config: {}", config_path.display())?;
    Ok(())
}

fn sources_list_remote(mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let config = config::Config::load_from(&config_path);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_sources_payload(&mut out, &config.sources, &config_path, mode).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write sources output: {e}"))
    })?;
    if config.sources.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn sources_add_remote(
    name: &str,
    host: &str,
    path: &str,
    transport: config::Transport,
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err(
            ErrorEnvelope::new("usage", "source name must not be empty")
                .with_hint("Pick a stable identifier, e.g. `laptop` or `prod-box`."),
        );
    }
    if host.trim().is_empty() {
        return Err(ErrorEnvelope::new("usage", "--host must not be empty"));
    }
    if path.trim().is_empty() {
        return Err(ErrorEnvelope::new("usage", "--path must not be empty"));
    }

    let config_path = resolve_config_path()?;
    let mut config = config::Config::load_from(&config_path);
    if config.sources.iter().any(|s| s.name == trimmed_name) {
        return Err(
            ErrorEnvelope::new(
                "duplicate-source",
                format!("a source named '{trimmed_name}' already exists"),
            )
            .with_hint("Use `aghist sources remove <name>` first, or pick a different name."),
        );
    }

    let new_source = config::RemoteSource {
        name: trimmed_name.to_string(),
        host: host.to_string(),
        path: path.to_string(),
        transport,
    };
    config.sources.push(new_source.clone());
    config.save_to(&config_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to write {}: {e}", config_path.display()),
        )
    })?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({
            "added": new_source,
            "config_path": config_path.display().to_string(),
        });
        serde_json::to_writer(&mut out, &payload).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}"))
        })?;
        writeln!(out).ok();
    } else {
        writeln!(
            out,
            "Added source '{}' ({} {}:{})",
            new_source.name,
            new_source.transport.slug(),
            new_source.host,
            new_source.path
        )
        .ok();
        writeln!(out, "Config: {}", config_path.display()).ok();
    }
    Ok(EXIT_OK)
}

fn sources_remove_remote(name: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;
    let config_path = resolve_config_path()?;
    let mut config = config::Config::load_from(&config_path);
    let before = config.sources.len();
    let mut removed: Option<config::RemoteSource> = None;
    config.sources.retain(|s| {
        if s.name == name {
            removed = Some(s.clone());
            false
        } else {
            true
        }
    });
    if config.sources.len() == before {
        return Err(
            ErrorEnvelope::new(
                "source-not-found",
                format!("no registered source named '{name}'"),
            )
            .with_hint("Run `aghist sources list` to see registered sources."),
        );
    }
    config.save_to(&config_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to write {}: {e}", config_path.display()),
        )
    })?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let removed = removed.expect("retain reported a removal");
    if mode.is_machine() {
        let payload = serde_json::json!({
            "removed": removed,
            "config_path": config_path.display().to_string(),
        });
        serde_json::to_writer(&mut out, &payload).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}"))
        })?;
        writeln!(out).ok();
    } else {
        writeln!(out, "Removed source '{}'", removed.name).ok();
        writeln!(out, "Config: {}", config_path.display()).ok();
    }
    Ok(EXIT_OK)
}

fn resolve_sources_cache_root() -> Result<PathBuf, ErrorEnvelope> {
    config::sources_cache_root().ok_or_else(|| {
        ErrorEnvelope::new(
            "config-error",
            "could not determine sources cache dir; HOME and XDG_CACHE_HOME are unset",
        )
        .with_hint("Set AGHIST_SOURCES_CACHE_DIR=/path/to/cache to override.")
    })
}

#[derive(serde::Serialize)]
struct PullResult {
    name: String,
    host: String,
    path: String,
    transport: String,
    data_dir: String,
    dry_run: bool,
    byte_count: u64,
    file_count: u64,
    pulled_at: DateTime<Utc>,
}

fn sources_pull_remote(
    name: Option<&str>,
    all: bool,
    dry_run: bool,
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;

    let config_path = resolve_config_path()?;
    let config = config::Config::load_from(&config_path);

    let targets: Vec<config::RemoteSource> = match (name, all) {
        (Some(n), false) => {
            let trimmed = n.trim();
            let Some(found) = config.sources.iter().find(|s| s.name == trimmed) else {
                return Err(ErrorEnvelope::new(
                    "source-not-found",
                    format!("no registered source named '{trimmed}'"),
                )
                .with_hint("Run `aghist sources list` to see registered sources."));
            };
            vec![found.clone()]
        }
        (None, true) => {
            if config.sources.is_empty() {
                return Err(ErrorEnvelope::new(
                    "source-not-found",
                    "no remote sources are registered",
                )
                .with_hint(
                    "Add one with `aghist sources add <name> --host <host> --path <path>`.",
                ));
            }
            config.sources.clone()
        }
        (None, false) => {
            return Err(ErrorEnvelope::new(
                "usage",
                "must pass either <NAME> or --all",
            )
            .with_hint("Run `aghist sources pull --help` for usage."));
        }
        (Some(_), true) => {
            // Clap rejects this combination via `conflicts_with`; defensive only.
            return Err(ErrorEnvelope::new(
                "usage",
                "<NAME> and --all are mutually exclusive",
            ));
        }
    };

    let cache_root = resolve_sources_cache_root()?;
    let mut results = Vec::with_capacity(targets.len());
    for src in targets {
        let result = pull_one_source(&src, &cache_root, dry_run)?;
        results.push(result);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_pull_results(&mut out, &results, &cache_root, mode).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write pull output: {e}"))
    })?;
    let _ = out.flush();
    Ok(EXIT_OK)
}

fn pull_one_source(
    src: &config::RemoteSource,
    cache_root: &std::path::Path,
    dry_run: bool,
) -> Result<PullResult, ErrorEnvelope> {
    let data_dir = src.data_dir(cache_root);
    std::fs::create_dir_all(&data_dir).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to create cache dir {}: {e}", data_dir.display()),
        )
    })?;

    let rsync_bin =
        std::env::var("AGHIST_RSYNC_BIN").unwrap_or_else(|_| "rsync".to_string());
    let remote = build_rsync_remote_url(src);
    // rsync expects a trailing slash on the dest to copy into the dir.
    let mut local = data_dir.display().to_string();
    if !local.ends_with('/') {
        local.push('/');
    }

    let mut cmd = std::process::Command::new(&rsync_bin);
    cmd.arg("-a").arg("--delete");
    if dry_run {
        cmd.arg("--dry-run");
    }
    if matches!(src.transport, config::Transport::Ssh) {
        // BatchMode=yes refuses interactive prompts; polecats and CI can't answer.
        cmd.arg("-e").arg("ssh -o BatchMode=yes");
    }
    cmd.arg(&remote).arg(&local);

    let output = cmd.output().map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to invoke rsync ('{rsync_bin}'): {e}"),
        )
        .with_hint("Install rsync, or set AGHIST_RSYNC_BIN to a working binary.")
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output
            .status
            .code()
            .map_or_else(|| String::from("?"), |c| c.to_string());
        return Err(ErrorEnvelope::new(
            "rsync-failed",
            format!("rsync exited {code} for source '{}'", src.name),
        )
        .with_hint(format!(
            "remote: {remote} — stderr: {}",
            stderr.lines().last().unwrap_or("").trim()
        )));
    }

    let (file_count, byte_count) = if dry_run {
        (0, 0)
    } else {
        count_dir(&data_dir)
    };
    let pulled_at = Utc::now();
    let manifest = config::SourceCacheManifest {
        name: src.name.clone(),
        host: src.host.clone(),
        path: src.path.clone(),
        transport: src.transport,
        data_dir: data_dir.display().to_string(),
        last_pulled_at: pulled_at,
        last_pull_dry_run: dry_run,
        byte_count,
        file_count,
    };
    let manifest_path = src.manifest_path(cache_root);
    manifest.save(&manifest_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!(
                "failed to write manifest {}: {e}",
                manifest_path.display()
            ),
        )
    })?;

    Ok(PullResult {
        name: src.name.clone(),
        host: src.host.clone(),
        path: src.path.clone(),
        transport: src.transport.slug().to_string(),
        data_dir: data_dir.display().to_string(),
        dry_run,
        byte_count,
        file_count,
        pulled_at,
    })
}

/// Build the rsync source URL for a remote source. `Ssh` transport uses
/// `host:path/` (rsync over SSH); `Rsync` uses `rsync://host/path/` (rsync
/// daemon protocol). Both end with a trailing slash so rsync copies the
/// directory contents rather than the directory itself.
fn build_rsync_remote_url(src: &config::RemoteSource) -> String {
    let path = src.path.trim_end_matches('/');
    match src.transport {
        config::Transport::Ssh => format!("{}:{}/", src.host, path),
        config::Transport::Rsync => {
            let path = path.trim_start_matches('/');
            format!("rsync://{}/{}/", src.host, path)
        }
    }
}

/// Recursive `(file_count, total_bytes)`. Symlinks and IO errors are skipped.
fn count_dir(dir: &std::path::Path) -> (u64, u64) {
    let mut files: u64 = 0;
    let mut bytes: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() {
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(meta.len());
        } else if meta.is_dir() {
            let (f, b) = count_dir(&entry.path());
            files = files.saturating_add(f);
            bytes = bytes.saturating_add(b);
        }
    }
    (files, bytes)
}

fn write_pull_results<W: io::Write>(
    out: &mut W,
    results: &[PullResult],
    cache_root: &std::path::Path,
    mode: OutputMode,
) -> io::Result<()> {
    match mode {
        OutputMode::Human => {
            if results.is_empty() {
                writeln!(out, "No sources pulled.")?;
                return Ok(());
            }
            writeln!(
                out,
                "{:<20}  {:<8}  {:<10}  {:<6}  PATH",
                "NAME", "FILES", "SIZE", "DRY"
            )?;
            for r in results {
                writeln!(
                    out,
                    "{:<20}  {:<8}  {:<10}  {:<6}  {}",
                    r.name,
                    r.file_count,
                    format_bytes(r.byte_count),
                    if r.dry_run { "yes" } else { "no" },
                    r.data_dir
                )?;
            }
            writeln!(out)?;
            writeln!(out, "Cache: {}", cache_root.display())?;
            Ok(())
        }
        OutputMode::Json => {
            let payload = serde_json::json!({
                "results": results,
                "cache_dir": cache_root.display().to_string(),
            });
            serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
            writeln!(out)
        }
        OutputMode::Ndjson => {
            for r in results {
                serde_json::to_writer(&mut *out, r).map_err(std::io::Error::other)?;
                writeln!(out)?;
            }
            Ok(())
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn format_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b >= GB {
        format!("{:.1}G", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1}M", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.1}K", b as f64 / KB as f64)
    } else {
        format!("{b}B")
    }
}

fn todos_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    kinds: &[TodoKind],
    limit: usize,
    force_json: bool,
    use_llm: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut all: Vec<TodoCandidate> = Vec::new();
    // For --llm: per-session metadata (project + started_at) keyed by
    // (provider, session_id). Built alongside `all` so we don't re-iterate
    // providers/sessions a second time in the LLM branch.
    let mut session_meta: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        (Option<String>, DateTime<Utc>),
    > = std::collections::HashMap::new();

    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            if !session_matches(&session, filters, project_needle.as_deref()) {
                continue;
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            let candidates =
                todos::extract_from_messages(p.provider(), &session.id, &messages, kinds);
            let mut session_emitted = false;
            for c in candidates {
                if filters.role.is_some() || filters.has_tool_call {
                    let turn_idx = (c.citation.turn as usize).saturating_sub(1);
                    let Some(msg) = messages.get(turn_idx) else {
                        continue;
                    };
                    if !message_matches(msg, filters) {
                        continue;
                    }
                }
                if let Some(since) = filters.since {
                    if c.timestamp < since {
                        continue;
                    }
                }
                if let Some(until) = filters.until {
                    if c.timestamp > until {
                        continue;
                    }
                }
                if use_llm && !session_emitted {
                    session_meta.insert(
                        (p.provider(), session.id.clone()),
                        (session.project_name.clone(), session.started_at),
                    );
                    session_emitted = true;
                }
                all.push(c);
            }
        }
    }

    if use_llm {
        return run_llm_todos(all, session_meta, limit, force_json, llm_model);
    }

    // Newest matches first — most useful for "what's still hanging?".
    all.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
            .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
    });

    if limit > 0 && all.len() > limit {
        all.truncate(limit);
    }

    if all.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_todos_json(&mut out, &all)
    } else {
        render_todos_human(&mut out, &all)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write todos output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_todos_json<W: io::Write>(out: &mut W, todos: &[TodoCandidate]) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": todos.iter().map(|c| serde_json::json!({
            "ref": c.citation.to_string(),
            "provider": c.citation.provider,
            "session_id": c.citation.session_id.0,
            "turn": c.citation.turn,
            "kind": c.kind,
            "snippet": c.snippet,
            "role": c.role,
            "timestamp": c.timestamp,
            "bd_id": c.bd_id,
        })).collect::<Vec<_>>(),
        "count": todos.len(),
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_todos_human<W: io::Write>(out: &mut W, todos: &[TodoCandidate]) -> io::Result<()> {
    writeln!(
        out,
        "{:<14}  {:<19}  {:<46}  SNIPPET",
        "KIND", "WHEN (UTC)", "REF"
    )?;
    for c in todos {
        let when = c.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let reference = c.citation.to_string();
        let reference = truncate(&reference, 46);
        let snippet = truncate(&c.snippet, 80);
        writeln!(
            out,
            "{:<14}  {:<19}  {:<46}  {snippet}",
            c.kind.slug(),
            when,
            reference
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} candidate(s)", todos.len())?;
    Ok(())
}

/// LLM-mode todo row with full metadata for rendering.
struct LlmTodoRow {
    citation: CitationRef,
    todo: aghist::llm::StructuredTodo,
    source_snippet: Option<String>,
    source_kind: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

/// Route heuristic candidates through the LLM and emit structured todos.
///
/// Groups candidates by `(provider, session_id)` and issues one Messages
/// API call per session. The system prompt is cache-controlled so calls
/// 2..N pay near-zero on the static prompt tokens. Falls through to
/// `EXIT_EMPTY` when no todos survive.
fn run_llm_todos(
    candidates: Vec<TodoCandidate>,
    session_meta: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        (Option<String>, DateTime<Utc>),
    >,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if candidates.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    // Group candidates by (provider, session_id), preserving first-seen
    // order so the API call sequence stays predictable across runs.
    let mut order: Vec<(Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        Vec<TodoCandidate>,
    > = std::collections::HashMap::new();
    for c in candidates {
        let key = (c.citation.provider, c.citation.session_id.clone());
        if !grouped.contains_key(&key) {
            order.push(key.clone());
        }
        grouped.entry(key).or_default().push(c);
    }

    let mut out: Vec<LlmTodoRow> = Vec::new();
    for key in order {
        let group = grouped.remove(&key).unwrap_or_default();
        let (project, started_at) = session_meta
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (None, DateTime::<Utc>::from_timestamp(0, 0).unwrap()));
        let llm_candidates: Vec<aghist::llm::TodoCandidate> = group
            .iter()
            .map(|c| aghist::llm::TodoCandidate {
                turn: c.citation.turn,
                role: c.role,
                kind: c.kind.slug(),
                snippet: c.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::TodoExtractionInput {
            provider: key.0,
            session_id: &key.1,
            project: project.as_deref(),
            candidates: llm_candidates,
        };
        let extracted = aghist::llm::extract_for_session_todos(&transport, &config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for et in extracted {
            out.push(LlmTodoRow {
                citation: et.citation,
                todo: et.todo,
                source_snippet: et.source_snippet,
                source_kind: et.source_kind,
                project: project.clone(),
                started_at,
            });
        }
    }

    out.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
    });
    if limit > 0 && out.len() > limit {
        out.truncate(limit);
    }
    if out.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut sink = stdout.lock();
    if want_json {
        render_llm_todos_json(&mut sink, &out)
    } else {
        render_llm_todos_human(&mut sink, &out)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write todos output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_llm_todos_json<W: io::Write>(out: &mut W, rows: &[LlmTodoRow]) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": rows.iter().map(|r| serde_json::json!({
            "ref": r.citation.to_string(),
            "provider": r.citation.provider,
            "session_id": r.citation.session_id.0,
            "turn": r.citation.turn,
            "description": r.todo.description,
            "target_session": r.todo.target_session,
            "status_inferred": r.todo.status_inferred,
            "source_snippet": r.source_snippet,
            "source_kind": r.source_kind,
            "project": r.project,
            "started_at": r.started_at,
        })).collect::<Vec<_>>(),
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_llm_todos_human<W: io::Write>(out: &mut W, rows: &[LlmTodoRow]) -> io::Result<()> {
    writeln!(
        out,
        "{:<8}  {:<46}  {:<48}  TARGET",
        "STATUS", "REF", "DESCRIPTION"
    )?;
    for r in rows {
        let status = match r.todo.status_inferred {
            aghist::llm::TodoStatus::Open => "open",
            aghist::llm::TodoStatus::Done => "done",
            aghist::llm::TodoStatus::Unclear => "unclear",
        };
        let reference = r.citation.to_string();
        let reference = truncate(&reference, 46);
        let description = truncate(&r.todo.description, 48);
        let target = r.todo.target_session.as_deref().unwrap_or("");
        writeln!(out, "{status:<8}  {reference:<46}  {description:<48}  {target}")?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} todo(s)", rows.len())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn threads_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    gap_hours: i64,
    min_sessions: usize,
    limit: usize,
    force_json: bool,
    use_llm: bool,
    llm_model: Option<&str>,
    llm_max_sessions: usize,
) -> Result<i32, ErrorEnvelope> {
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }
    if gap_hours < 0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--gap-hours must be >= 0 (got {gap_hours})"),
        ));
    }

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut sessions: Vec<Session> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(found) => sessions.extend(
                found
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref())),
            ),
            Err(e) => eprintln!("{}: error: {e}", p.provider()),
        }
    }

    if use_llm {
        return run_llm_threads(sessions, limit, llm_max_sessions, force_json, llm_model);
    }

    let opts = aghist::threads::ClusterOptions {
        gap: chrono::Duration::hours(gap_hours),
        min_sessions: min_sessions.max(1),
    };
    let mut threads = aghist::threads::cluster(&sessions, opts);

    if limit > 0 && threads.len() > limit {
        threads.truncate(limit);
    }

    if threads.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_threads_json(&mut out, &threads)
    } else {
        render_threads_human(&mut out, &threads)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write threads output: {e}")))?;

    Ok(EXIT_OK)
}

/// LLM-driven topic clustering. Builds one session digest per local
/// `Session`, caps to the most recent `llm_max_sessions`, and routes
/// everything through a single Messages API call. Augments each returned
/// thread with derived metadata (providers / projects / `message_count`)
/// so the JSON shape stays compatible with the heuristic where possible.
fn run_llm_threads(
    sessions: Vec<Session>,
    limit: usize,
    llm_max_sessions: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if sessions.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    // Cap to most-recent `llm_max_sessions` so input tokens stay bounded.
    // 0 means "no cap"; mirrors the rest of the CLI.
    let mut sorted = sessions;
    sorted.sort_by_key(|s| std::cmp::Reverse(s.started_at));
    if llm_max_sessions > 0 && sorted.len() > llm_max_sessions {
        sorted.truncate(llm_max_sessions);
    }

    let digests: Vec<aghist::llm::SessionDigest> = sorted
        .iter()
        .map(|s| aghist::llm::SessionDigest {
            provider: s.provider,
            session_id: s.id.clone(),
            project: s.project_name.clone(),
            started_at: s.started_at,
            ended_at: s.ended_at,
            summary: s.summary.clone(),
        })
        .collect();

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let raw = aghist::llm::extract_threads(&transport, &config, &digests)
        .map_err(|e| map_llm_error(&e))?;

    // Index sessions by `<provider-slug>/<session-id>` so we can stitch
    // derived metadata (providers, message_count, ...) onto each thread.
    let mut by_ref: std::collections::HashMap<String, &Session> =
        std::collections::HashMap::with_capacity(sorted.len());
    for s in &sorted {
        by_ref.insert(format!("{}/{}", s.provider.slug(), s.id.0), s);
    }

    let mut rows: Vec<LlmThreadRow> = Vec::with_capacity(raw.len());
    for t in raw {
        let mut providers_set: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
        let mut branches_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut projects_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut message_count: usize = 0;
        for r in &t.member_refs {
            if let Some(s) = by_ref.get(r) {
                providers_set.insert(s.provider.slug());
                if let Some(b) = s.git_branch.as_deref().filter(|x| !x.is_empty()) {
                    branches_set.insert(b.to_string());
                }
                if let Some(p) = s.project_name.as_deref() {
                    projects_set.insert(p.to_string());
                }
                message_count = message_count.saturating_add(s.message_count);
            }
        }
        let id = llm_thread_id(&t.topic_summary, t.member_refs.first().map(String::as_str));
        rows.push(LlmThreadRow {
            id,
            topic_summary: t.topic_summary,
            member_refs: t.member_refs,
            time_span: t.time_span,
            providers: providers_set.into_iter().map(str::to_string).collect(),
            projects: projects_set.into_iter().collect(),
            branches: branches_set.into_iter().collect(),
            message_count,
        });
    }

    rows.sort_by(|a, b| {
        b.time_span
            .start
            .cmp(&a.time_span.start)
            .then_with(|| a.topic_summary.cmp(&b.topic_summary))
    });
    if limit > 0 && rows.len() > limit {
        rows.truncate(limit);
    }
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_llm_threads_json(&mut out, &rows)
    } else {
        render_llm_threads_human(&mut out, &rows)
    }
    .map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write threads output: {e}"))
    })?;

    Ok(EXIT_OK)
}

/// Stable short id for an LLM-grouped thread: FNV-1a over
/// `<topic_summary>|<first_member_ref>`. Mirrors the heuristic id format
/// (`th-<hex16>`) so consumers can format-discriminate.
fn llm_thread_id(topic: &str, first_ref: Option<&str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in topic.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^= u64::from(b'|');
    h = h.wrapping_mul(0x100_0000_01b3);
    for byte in first_ref.unwrap_or("").as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("th-{h:016x}")
}

#[derive(serde::Serialize)]
struct LlmThreadRow {
    id: String,
    topic_summary: String,
    member_refs: Vec<String>,
    time_span: aghist::llm::TimeSpan,
    providers: Vec<String>,
    projects: Vec<String>,
    branches: Vec<String>,
    message_count: usize,
}

fn render_llm_threads_json<W: io::Write>(
    out: &mut W,
    rows: &[LlmThreadRow],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "threads": rows,
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_llm_threads_human<W: io::Write>(
    out: &mut W,
    rows: &[LlmThreadRow],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  TOPIC",
        "START (UTC)", "END (UTC)", "SESS", "MSGS", "ID"
    )?;
    for r in rows {
        let started = r.time_span.start.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = r.time_span.end.format("%Y-%m-%d %H:%M:%S").to_string();
        let topic = truncate(&r.topic_summary, 60);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {topic}",
            started,
            ended,
            r.member_refs.len(),
            r.message_count,
            truncate(&r.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", rows.len())?;
    Ok(())
}

fn render_threads_json<W: io::Write>(
    out: &mut W,
    threads: &[aghist::threads::Thread],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "threads": threads,
        "count": threads.len(),
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_threads_human<W: io::Write>(
    out: &mut W,
    threads: &[aghist::threads::Thread],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  PROJECT",
        "STARTED (UTC)", "ENDED (UTC)", "SESS", "MSGS", "ID"
    )?;
    for t in threads {
        let started = t.started_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = t.ended_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let project = t.project.as_deref().unwrap_or("(unknown)");
        let project = truncate(project, 40);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {project}",
            started,
            ended,
            t.session_count,
            t.message_count,
            truncate(&t.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", threads.len())?;
    Ok(())
}

fn usage_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    group_by: aghist::usage::GroupBy,
    limit: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut sessions: Vec<Session> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(found) => sessions.extend(
                found
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref())),
            ),
            Err(e) => eprintln!("{}: error: {e}", p.provider()),
        }
    }

    let report = aghist::usage::aggregate(&sessions, group_by);
    if report.rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let total_rows = report.rows.len();
    let trimmed = if limit > 0 && total_rows > limit {
        let mut r = report;
        r.rows.truncate(limit);
        r
    } else {
        report
    };

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_usage_json(&mut out, &trimmed, group_by, total_rows)
    } else {
        render_usage_human(&mut out, &trimmed, group_by, total_rows)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write usage output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_usage_json<W: io::Write>(
    out: &mut W,
    report: &aghist::usage::UsageReport,
    group_by: aghist::usage::GroupBy,
    total_rows: usize,
) -> io::Result<()> {
    let payload = serde_json::json!({
        "rows": report.rows,
        "totals": report.totals,
        "meta": {
            "group_by": group_by.as_str(),
            "row_count": report.rows.len(),
            "total_row_count": total_rows,
        },
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_usage_human<W: io::Write>(
    out: &mut W,
    report: &aghist::usage::UsageReport,
    group_by: aghist::usage::GroupBy,
    total_rows: usize,
) -> io::Result<()> {
    let key_header = match group_by {
        aghist::usage::GroupBy::Model => "MODEL",
        aghist::usage::GroupBy::Provider => "PROVIDER",
        aghist::usage::GroupBy::Project => "PROJECT",
    };
    writeln!(
        out,
        "{:<32}  {:>5}  {:>12}  {:>12}  {:>14}  COST",
        key_header, "SESS", "INPUT", "OUTPUT", "TOTAL TOKENS"
    )?;
    for row in &report.rows {
        let cost = row
            .cost_usd
            .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
        writeln!(
            out,
            "{:<32}  {:>5}  {:>12}  {:>12}  {:>14}  {cost}",
            truncate(&row.key, 32),
            row.session_count,
            row.input_tokens,
            row.output_tokens,
            row.total_tokens,
        )?;
    }
    writeln!(out)?;
    let total_cost = report
        .totals
        .cost_usd
        .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
    writeln!(
        out,
        "Total: {} session(s), {} message(s), {} token(s), cost {}",
        report.totals.session_count,
        report.totals.message_count,
        report.totals.total_tokens,
        total_cost,
    )?;
    if total_rows > report.rows.len() {
        writeln!(
            out,
            "(showing {} of {} row(s) — pass --limit 0 for all)",
            report.rows.len(),
            total_rows,
        )?;
    }
    Ok(())
}

fn project_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    name: &str,
    limits: aghist::project::ProjectLimits,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let needle = name.trim();
    if needle.is_empty() {
        ErrorEnvelope::new("usage", "project <name> must not be empty").emit();
        return Ok(EXIT_USAGE);
    }
    let needle_lower = needle.to_lowercase();
    // The global `--project` filter, if set, AND-narrows the positional name —
    // both must match. Useful for agents combining a saved alias with an
    // ad-hoc query.
    let extra_project = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut bundles: Vec<(Session, Vec<Message>)> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            // Reuse session-level filters (since/until/note/tag/starred) by
            // routing through the shared helper. The positional `name` is
            // applied on top so the global `--project` substring still works.
            if !session_matches(&session, filters, extra_project.as_deref()) {
                continue;
            }
            let project_name = session.project_name.as_deref().unwrap_or("");
            if !project_name.to_lowercase().contains(&needle_lower) {
                continue;
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            bundles.push((session, messages));
        }
    }

    if bundles.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let report = aghist::project::aggregate(needle, &bundles, limits);

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_project_json(&mut out, &report)
    } else {
        render_project_human(&mut out, &report)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write project output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_project_json<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    serde_json::to_writer(&mut *out, report).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn render_project_human<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(out, "Project: {}", report.query)?;
    if !report.matched_projects.is_empty() {
        writeln!(out, "Matched: {}", report.matched_projects.join(", "))?;
    }
    let cost = report
        .token_usage
        .cost_usd
        .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
    writeln!(
        out,
        "{} session(s), {} message(s), {} token(s), cost {cost}",
        report.session_count, report.message_count, report.token_usage.total_tokens,
    )?;
    if let (Some(start), Some(end)) = (report.started_at, report.ended_at) {
        writeln!(
            out,
            "Active: {} → {}",
            start.format("%Y-%m-%d %H:%M:%SZ"),
            end.format("%Y-%m-%d %H:%M:%SZ"),
        )?;
    }
    writeln!(out)?;

    writeln!(
        out,
        "Tokens: in {} | out {} | cache_r {} | cache_w {}",
        report.token_usage.input_tokens,
        report.token_usage.output_tokens,
        report.token_usage.cache_read_tokens,
        report.token_usage.cache_write_tokens,
    )?;
    writeln!(out)?;

    writeln!(
        out,
        "Decisions ({} of {}):",
        report.decisions.len(),
        report.meta.decisions_total,
    )?;
    if report.decisions.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for d in &report.decisions {
        let snippet = truncate(&d.snippet, 80);
        writeln!(
            out,
            "  [{:>4.1}] {}  {snippet}",
            d.score,
            truncate(&d.reference, 36),
        )?;
    }
    writeln!(out)?;

    writeln!(
        out,
        "Todos ({} of {}):",
        report.todos.len(),
        report.meta.todos_total,
    )?;
    if report.todos.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for t in &report.todos {
        let snippet = truncate(&t.snippet, 80);
        writeln!(
            out,
            "  [{:<12}] {}  {snippet}",
            t.kind.slug(),
            truncate(&t.reference, 36),
        )?;
    }
    writeln!(out)?;

    writeln!(
        out,
        "Threads ({} of {}; gap {}h):",
        report.threads.len(),
        report.meta.threads_total,
        report.meta.thread_gap_hours,
    )?;
    if report.threads.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for t in &report.threads {
        writeln!(
            out,
            "  {} → {}  {} session(s), {} msg(s)",
            t.started_at.format("%Y-%m-%d %H:%M"),
            t.ended_at.format("%Y-%m-%d %H:%M"),
            t.session_count,
            t.message_count,
        )?;
    }
    writeln!(out)?;

    writeln!(
        out,
        "Top files ({} of {}):",
        report.top_files.len(),
        report.meta.files_total,
    )?;
    if report.top_files.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for f in &report.top_files {
        writeln!(out, "  {:>6}  {}", f.count, truncate(&f.path, 70))?;
    }
    writeln!(out)?;

    writeln!(out, "Time of day (UTC, message counts):")?;
    let max = *report.time_of_day.iter().max().unwrap_or(&0);
    for (h, count) in report.time_of_day.iter().enumerate() {
        let bar_len = bar_cells(*count, max, 20);
        let bar = "█".repeat(bar_len);
        writeln!(out, "  {h:02}:00  {count:>6}  {bar}")?;
    }
    Ok(())
}

fn report_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    window_days: i64,
    limits: aghist::report::ReportLimits,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;
    // Resolve the window: --days/--week/--month set the default span, but
    // the global --since/--until override the computed start/end so users
    // can still pin an exact range.
    let now = Utc::now();
    let end = filters.until.unwrap_or(now);
    let start = filters
        .since
        .unwrap_or_else(|| end - chrono::Duration::days(window_days.max(1)));
    if end < start {
        return Err(ErrorEnvelope::new(
            "usage",
            "--until must be greater than or equal to --since",
        )
        .with_hint("Pass timestamps in chronological order, or rely on --days."));
    }
    let window = aghist::report::ReportWindow::between(start, end);

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    // Reuse the global filter machinery for since/until/project/provider so
    // the report sees the same sessions any other subcommand would for the
    // same flag set. We additionally clip to [start, end] in case the
    // caller did not pass --since/--until.
    let mut bundles: Vec<(Session, Vec<Message>)> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            if !session_matches(&session, filters, project_needle.as_deref()) {
                continue;
            }
            // Always clip to the resolved window — protects against the
            // common case where neither --since nor --until is set.
            if session.started_at < start || session.started_at > end {
                continue;
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            bundles.push((session, messages));
        }
    }

    if bundles.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let envelope = aghist::report::aggregate(window, &bundles, limits);

    // The spec calls for "Markdown output suitable for pasting into a
    // journal or weekly review", so `report` deviates from the rest of the
    // CLI and defaults to Markdown for both TTY and pipe. `--json` exists
    // for agents that need to consume the structured envelope.
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if force_json {
        serde_json::to_writer(&mut out, &envelope)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to encode report: {e}")))?;
        writeln!(out)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write report: {e}")))?;
    } else {
        let md = aghist::report::render_markdown(&envelope);
        write!(out, "{md}")
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write report: {e}")))?;
    }
    Ok(EXIT_OK)
}

/// Scale a `count` to a 0..=`max_cells` bar width relative to `max`.
/// Saturates to `max_cells` so a one-off outlier doesn't blow the layout
/// and rounds to the nearest cell so small bars round up rather than vanish.
#[allow(clippy::cast_precision_loss)] // u64 → f64: counts are message tallies, fit easily
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_wrap)]
fn bar_cells(count: u64, max: u64, max_cells: u64) -> usize {
    if max == 0 || count == 0 {
        return 0;
    }
    // Cap the ratio at 1.0 so saturation is explicit.
    let ratio = (count as f64 / max as f64).min(1.0);
    let cells = (ratio * max_cells as f64).round() as u64;
    cells.min(max_cells) as usize
}

fn show_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw_ref: &str,
    format: ShowFormat,
    include_context: u32,
) -> Result<i32, ErrorEnvelope> {
    let citation: CitationRef = raw_ref.parse().map_err(|e: aghist::model::CitationParseError| {
        ErrorEnvelope::new("usage", format!("invalid ref '{raw_ref}': {e}"))
            .with_hint("Format: <provider-slug>/<session-id>#<turn>. Example: claude-code/abc-123#7")
    })?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == citation.provider)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!("provider '{}' is not enabled or not detected", citation.provider.slug()),
            )
            .with_hint("Enable it in your config (`providers` table) or check that the source dir exists.")
        })?;

    let sessions = provider.discover_sessions().map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to discover sessions for {}: {e}", citation.provider.slug()),
        )
    })?;

    let session = sessions
        .iter()
        .find(|s| s.id == citation.session_id)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!(
                    "session '{}' not found in provider '{}'",
                    citation.session_id, citation.provider.slug()
                ),
            )
            .with_hint("Run `aghist --list` to see available session IDs.")
        })?;

    let messages = provider.load_messages(session).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let total = messages.len();
    let turn = citation.turn as usize;
    if turn > total {
        return Err(ErrorEnvelope::new(
            "session-not-found",
            format!("turn {turn} out of range: session has {total} message(s)"),
        )
        .with_hint("Use `aghist export` to inspect the full session, or pick a smaller turn."));
    }

    let target_idx = turn - 1; // turn is 1-based, idx is 0-based
    let ctx = include_context as usize;
    let start_idx = target_idx.saturating_sub(ctx);
    let end_idx = (target_idx + ctx + 1).min(total);
    let slice = &messages[start_idx..end_idx];

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        ShowFormat::Md => render_show_md(&mut out, &citation, session, slice, start_idx, target_idx),
        ShowFormat::Json => render_show_json(&mut out, &citation, session, slice, start_idx, target_idx),
        ShowFormat::Text => render_show_text(&mut out, &citation, slice, start_idx, target_idx),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write show output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_show_md<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "# {citation}")?;
    if let Some(project) = &session.project_name {
        writeln!(out, "_{project}_")?;
    }
    writeln!(out)?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx { " ←" } else { "" };
        writeln!(out, "## Turn {turn_no} — {}{marker}\n", msg.role)?;
        write_blocks_text(out, msg)?;
        writeln!(out)?;
    }
    Ok(())
}

fn render_show_json<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct ShowMessage<'a> {
        turn: usize,
        is_target: bool,
        role: Role,
        content: &'a [aghist::model::ContentBlock],
        timestamp: chrono::DateTime<chrono::Utc>,
    }
    #[derive(serde::Serialize)]
    struct ShowOut<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: Provider,
        session_id: &'a str,
        project: Option<&'a str>,
        target_turn: u32,
        messages: Vec<ShowMessage<'a>>,
    }

    let messages: Vec<ShowMessage> = slice
        .iter()
        .enumerate()
        .map(|(i, m)| ShowMessage {
            turn: start_idx + i + 1,
            is_target: start_idx + i == target_idx,
            role: m.role,
            content: &m.content,
            timestamp: m.timestamp,
        })
        .collect();

    let payload = ShowOut {
        reference: citation.to_string(),
        provider: citation.provider,
        session_id: session.id.0.as_str(),
        project: session.project_name.as_deref(),
        target_turn: citation.turn,
        messages,
    };

    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_show_text<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "{citation}")?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx { " (target)" } else { "" };
        writeln!(out, "--- Turn {turn_no} — {}{marker} ---", msg.role)?;
        write_blocks_text(out, msg)?;
    }
    Ok(())
}

fn write_blocks_text<W: io::Write>(out: &mut W, msg: &Message) -> io::Result<()> {
    use aghist::model::ContentBlock;
    for block in &msg.content {
        match block {
            ContentBlock::Text(t) => writeln!(out, "{t}")?,
            ContentBlock::CodeBlock { language, code } => {
                let lang = language.as_deref().unwrap_or("");
                writeln!(out, "```{lang}\n{code}\n```")?;
            }
            ContentBlock::ToolUse(tool) => {
                writeln!(out, "[tool: {}]\n{}", tool.name, tool.arguments)?;
            }
            ContentBlock::ToolResult(result) => {
                let status = if result.success { "ok" } else { "err" };
                writeln!(out, "[tool-result {status}]\n{}", result.output)?;
            }
            ContentBlock::Thinking(t) => writeln!(out, "[thinking] {t}")?,
            ContentBlock::Error(t) => writeln!(out, "[error] {t}")?,
        }
    }
    Ok(())
}

/// Run the heuristic across all matching sessions, returning unsorted rows.
fn collect_decision_rows(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    session_needle: Option<&str>,
    threshold: f32,
) -> Vec<DecisionRow> {
    let mut rows: Vec<DecisionRow> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            if !session_matches(&session, filters, project_needle) {
                continue;
            }
            if let Some(needle) = session_needle {
                if !session.id.0.starts_with(needle) {
                    continue;
                }
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            let scored: Vec<(usize, &Message)> = messages
                .iter()
                .enumerate()
                .filter(|(_, m)| message_matches(m, filters))
                .collect();
            for (idx, msg) in scored {
                let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                let cands = aghist::decisions::extract_from_message(msg, turn, threshold);
                for c in cands {
                    let Some(citation) = aghist::model::CitationRef::new(
                        session.provider,
                        session.id.clone(),
                        c.turn,
                    ) else {
                        continue;
                    };
                    rows.push(DecisionRow {
                        citation,
                        candidate: c,
                        project: session.project_name.clone(),
                        started_at: session.started_at,
                    });
                }
            }
        }
    }
    rows
}

// ── diff command ─────────────────────────────────────────────────────────────

/// A single "line" in the diff: the key used for LCS comparison and the
/// human-readable summary for rendering.
struct DiffLine {
    key: String,
    role: String,
    snippet: String,
}

impl DiffLine {
    fn from_message(msg: &Message) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };
        let snippet = first_text_snippet(msg, 120);
        let key = format!("{role}:{}", first_text_snippet(msg, 64));
        Self {
            key,
            role: role.to_string(),
            snippet,
        }
    }
}

fn first_text_snippet(msg: &Message, max: usize) -> String {
    for block in &msg.content {
        if let ContentBlock::Text(t) = block {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                return truncate(trimmed, max);
            }
        }
    }
    String::new()
}

/// LCS-based diff: returns edit-script as a list of `(in_a, in_b, key_idx_a, key_idx_b)`.
/// `true/false` means the line is present in that side.
enum DiffOp {
    Same(usize, usize),
    Delete(usize),
    Insert(usize),
}

fn lcs_diff(a: &[DiffLine], b: &[DiffLine]) -> Vec<DiffOp> {
    let m = a.len();
    let n = b.len();
    // DP table — O(m*n) space; sessions are short (hundreds of messages at most)
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i][j] = if a[i].key == b[j].key {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < m || j < n {
        if i < m && j < n && a[i].key == b[j].key {
            ops.push(DiffOp::Same(i, j));
            i += 1;
            j += 1;
        } else if j < n && (i >= m || dp[i + 1][j] >= dp[i][j + 1]) {
            ops.push(DiffOp::Insert(j));
            j += 1;
        } else {
            ops.push(DiffOp::Delete(i));
            i += 1;
        }
    }
    ops
}

/// Load a session by `<provider>/<session-id>` ref (no turn).
fn load_session_messages(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw: &str,
) -> Result<(Session, Vec<Message>), ErrorEnvelope> {
    let (slug, session_id) = raw.split_once('/').ok_or_else(|| {
        ErrorEnvelope::new("usage", format!("invalid session ref '{raw}': expected <provider>/<session-id>"))
    })?;
    let provider_kind = Provider::from_slug(slug).ok_or_else(|| {
        ErrorEnvelope::new("usage", format!("unknown provider slug '{slug}'"))
    })?;
    let p = providers
        .iter()
        .find(|p| p.provider() == provider_kind)
        .ok_or_else(|| {
            ErrorEnvelope::new("provider-unavailable", format!("provider '{slug}' not detected"))
        })?;
    let sessions = p.discover_sessions().map_err(|e| {
        ErrorEnvelope::new("provider-error", format!("discover {slug}: {e}"))
    })?;
    let session = sessions
        .into_iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| {
            ErrorEnvelope::new("session-not-found", format!("session '{session_id}' not found in {slug}"))
        })?;
    let messages = p.load_messages(&session).map_err(|e| {
        ErrorEnvelope::new("provider-error", format!("load {slug}/{session_id}: {e}"))
    })?;
    Ok((session, messages))
}

fn diff_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw1: &str,
    raw2: &str,
    context: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let (sess1, msgs1) = load_session_messages(providers, raw1)?;
    let (sess2, msgs2) = load_session_messages(providers, raw2)?;

    let lines1: Vec<DiffLine> = msgs1.iter().map(DiffLine::from_message).collect();
    let lines2: Vec<DiffLine> = msgs2.iter().map(DiffLine::from_message).collect();

    let ops = lcs_diff(&lines1, &lines2);
    let want_json = force_json || !io::stdout().is_terminal();

    if want_json {
        render_diff_json(raw1, raw2, &sess1, &sess2, &lines1, &lines2, &ops)?;
    } else {
        render_diff_text(raw1, raw2, &sess1, &sess2, &lines1, &lines2, &ops, context)?;
    }

    let has_changes = ops.iter().any(|o| !matches!(o, DiffOp::Same(_, _)));
    Ok(if has_changes { EXIT_OK } else { EXIT_EMPTY })
}

fn render_diff_text(
    raw1: &str,
    raw2: &str,
    sess1: &Session,
    sess2: &Session,
    lines1: &[DiffLine],
    lines2: &[DiffLine],
    ops: &[DiffOp],
    context: usize,
) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "--- {raw1}  ({}  {} msgs)", sess1.started_at.format("%Y-%m-%d"), lines1.len())
        .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    writeln!(out, "+++ {raw2}  ({}  {} msgs)", sess2.started_at.format("%Y-%m-%d"), lines2.len())
        .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;

    // Build a flat list of (marker, idx_a, idx_b) for hunk slicing
    struct FlatOp {
        marker: char,
        side_a: Option<usize>,
        side_b: Option<usize>,
    }
    let flat: Vec<FlatOp> = ops
        .iter()
        .map(|op| match op {
            DiffOp::Same(a, b) => FlatOp { marker: ' ', side_a: Some(*a), side_b: Some(*b) },
            DiffOp::Delete(a) => FlatOp { marker: '-', side_a: Some(*a), side_b: None },
            DiffOp::Insert(b) => FlatOp { marker: '+', side_a: None, side_b: Some(*b) },
        })
        .collect();

    // Identify hunk ranges (changed ops ± context)
    let changed: Vec<usize> = flat
        .iter()
        .enumerate()
        .filter(|(_, f)| f.marker != ' ')
        .map(|(i, _)| i)
        .collect();

    if changed.is_empty() {
        writeln!(out, "(sessions are identical)").ok();
        return Ok(());
    }

    // Merge overlapping hunk windows
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for &c in &changed {
        let start = c.saturating_sub(context);
        let end = (c + context + 1).min(flat.len());
        if let Some(last) = hunks.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        hunks.push((start, end));
    }

    for (hunk_start, hunk_end) in hunks {
        // Hunk header: count a/b lines
        let a_start = flat[hunk_start].side_a.unwrap_or(0) + 1;
        let b_start = flat[hunk_start].side_b.unwrap_or(0) + 1;
        let a_count = flat[hunk_start..hunk_end].iter().filter(|f| f.side_a.is_some()).count();
        let b_count = flat[hunk_start..hunk_end].iter().filter(|f| f.side_b.is_some()).count();
        writeln!(out, "@@ -{a_start},{a_count} +{b_start},{b_count} @@")
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        for f in &flat[hunk_start..hunk_end] {
            let line = match (f.side_a, f.side_b) {
                (Some(a), _) => &lines1[a],
                (None, Some(b)) => &lines2[b],
                _ => continue,
            };
            writeln!(out, "{}{}: {}", f.marker, line.role, line.snippet)
                .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        }
    }
    Ok(())
}

fn render_diff_json(
    raw1: &str,
    raw2: &str,
    sess1: &Session,
    sess2: &Session,
    lines1: &[DiffLine],
    lines2: &[DiffLine],
    ops: &[DiffOp],
) -> Result<(), ErrorEnvelope> {
    let entries: Vec<serde_json::Value> = ops
        .iter()
        .map(|op| match op {
            DiffOp::Same(a, b) => serde_json::json!({
                "op": "same",
                "role": lines1[*a].role,
                "snippet": lines1[*a].snippet,
                "turn_a": a + 1,
                "turn_b": b + 1,
            }),
            DiffOp::Delete(a) => serde_json::json!({
                "op": "delete",
                "role": lines1[*a].role,
                "snippet": lines1[*a].snippet,
                "turn_a": a + 1,
            }),
            DiffOp::Insert(b) => serde_json::json!({
                "op": "insert",
                "role": lines2[*b].role,
                "snippet": lines2[*b].snippet,
                "turn_b": b + 1,
            }),
        })
        .collect();

    let payload = serde_json::json!({
        "session1": {"ref": raw1, "started_at": sess1.started_at, "turns": lines1.len()},
        "session2": {"ref": raw2, "started_at": sess2.started_at, "turns": lines2.len()},
        "ops": entries,
        "changed": ops.iter().filter(|o| !matches!(o, DiffOp::Same(_, _))).count(),
        "same": ops.iter().filter(|o| matches!(o, DiffOp::Same(_, _))).count(),
    });
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &payload)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("json: {e}")))?;
    writeln!(out).map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decisions_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    session_filter: Option<&str>,
    threshold: f32,
    limit: usize,
    force_json: bool,
    filters: &FilterArgs,
    use_llm: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new(
            "usage",
            "--llm-model requires --llm",
        ));
    }
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--threshold must be a non-negative finite number (got {threshold})"),
        ));
    }

    // If --session was given as a full citation ref, drop the trailing
    // `#turn` so it can match the session id; we extract decisions across
    // the whole session regardless of the cited turn.
    let session_needle = session_filter.map(|s| {
        let trimmed = s.trim();
        let without_turn = trimmed.rsplit_once('#').map_or(trimmed, |(head, _)| head);
        // Strip leading provider segment if present (`<slug>/<id>` → `<id>`).
        without_turn
            .split_once('/')
            .map_or(without_turn, |(_, rest)| rest)
            .to_string()
    });

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut rows = collect_decision_rows(
        providers,
        filters,
        project_needle.as_deref(),
        session_needle.as_deref(),
        threshold,
    );

    rows.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.started_at.cmp(&a.started_at))
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.candidate.turn.cmp(&b.candidate.turn))
    });

    if use_llm {
        return run_llm_decisions(rows, limit, force_json, llm_model);
    }

    if rows.len() > limit {
        rows.truncate(limit);
    }

    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        print_decisions_json(&rows).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_decisions_table(&rows);
    }
    Ok(EXIT_OK)
}

/// Route heuristic candidates through the LLM and emit structured decisions.
///
/// Groups rows by `(provider, session_id)` and issues one Messages API call
/// per session. The system prompt is cache-controlled, so calls 2..N pay
/// near-zero on the static prompt tokens. Falls through to `EXIT_EMPTY` if
/// no decisions survive.
fn run_llm_decisions(
    rows: Vec<DecisionRow>,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let groups = group_by_session(rows);
    let mut out = run_extraction(&transport, &config, groups)?;

    out.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
    });
    if out.len() > limit {
        out.truncate(limit);
    }
    if out.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    if force_json || !io::stdout().is_terminal() {
        print_llm_decisions_json(&out).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_llm_decisions_table(&out);
    }
    Ok(EXIT_OK)
}

/// Group heuristic rows by `(provider, session_id)`, preserving first-seen
/// order so the API call sequence stays predictable.
fn group_by_session(rows: Vec<DecisionRow>) -> Vec<SessionGroup> {
    let mut order: Vec<(Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        SessionGroup,
    > = std::collections::HashMap::new();
    for row in rows {
        let key = (row.citation.provider, row.citation.session_id.clone());
        let entry = grouped.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            SessionGroup {
                provider: row.citation.provider,
                session_id: row.citation.session_id.clone(),
                project: row.project.clone(),
                started_at: row.started_at,
                candidates: Vec::new(),
            }
        });
        entry.candidates.push(GroupedCandidate {
            turn: row.candidate.turn,
            role: row.candidate.role,
            snippet: row.candidate.snippet,
        });
    }
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        if let Some(g) = grouped.remove(&key) {
            out.push(g);
        }
    }
    out
}

fn run_extraction<T: aghist::llm::LlmTransport + ?Sized>(
    transport: &T,
    config: &aghist::llm::LlmConfig,
    groups: Vec<SessionGroup>,
) -> Result<Vec<LlmRow>, ErrorEnvelope> {
    let mut out = Vec::new();
    for group in groups {
        let candidates: Vec<aghist::llm::Candidate> = group
            .candidates
            .iter()
            .map(|c| aghist::llm::Candidate {
                turn: c.turn,
                role: c.role,
                snippet: c.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::ExtractionInput {
            provider: group.provider,
            session_id: &group.session_id,
            project: group.project.as_deref(),
            candidates,
        };
        let extracted = aghist::llm::extract_for_session(transport, config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for ed in extracted {
            out.push(LlmRow {
                citation: ed.citation,
                decision: ed.decision,
                source_snippet: ed.source_snippet,
                project: group.project.clone(),
                started_at: group.started_at,
            });
        }
    }
    Ok(out)
}

struct SessionGroup {
    provider: Provider,
    session_id: aghist::model::SessionId,
    project: Option<String>,
    started_at: DateTime<Utc>,
    candidates: Vec<GroupedCandidate>,
}

struct GroupedCandidate {
    turn: u32,
    role: Role,
    snippet: String,
}

struct LlmRow {
    citation: CitationRef,
    decision: aghist::llm::StructuredDecision,
    source_snippet: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

fn map_llm_error(e: &aghist::llm::LlmError) -> ErrorEnvelope {
    use aghist::llm::LlmError;
    let env = ErrorEnvelope::new("llm-error", e.to_string());
    match e {
        LlmError::MissingApiKey => {
            env.with_hint("Set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) and re-run.")
        }
        LlmError::ApiStatus { status: 401 | 403, .. } => env.with_hint(
            "Verify ANTHROPIC_API_KEY is valid and has access to the chosen model.",
        ),
        LlmError::ApiStatus { status: 429, .. } => {
            env.with_hint("Rate limited — retry with --limit lowered or wait and retry.")
        }
        _ => env,
    }
}

fn print_llm_decisions_table(rows: &[LlmRow]) {
    println!(
        "{:<36}  {:<60}  RATIONALE",
        "REF", "SUMMARY"
    );
    for row in rows {
        let r = row.citation.to_string();
        let r = truncate(&r, 36);
        let summary = truncate(&row.decision.summary, 60);
        let rationale = truncate(&row.decision.rationale, 80);
        println!("{r:<36}  {summary:<60}  {rationale}");
    }
}

fn print_llm_decisions_json(rows: &[LlmRow]) -> std::io::Result<()> {
    use std::io::Write as _;

    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: aghist::model::Provider,
        session_id: &'a str,
        turn: u32,
        summary: &'a str,
        rationale: &'a str,
        alternatives: &'a [String],
        source_snippet: Option<&'a str>,
        project: Option<&'a str>,
        started_at: chrono::DateTime<chrono::Utc>,
    }

    #[derive(serde::Serialize)]
    struct Payload<'a> {
        decisions: Vec<JsonRow<'a>>,
        count: usize,
        mode: &'static str,
    }

    let decisions: Vec<JsonRow> = rows
        .iter()
        .map(|r| JsonRow {
            reference: r.citation.to_string(),
            provider: r.citation.provider,
            session_id: r.citation.session_id.0.as_str(),
            turn: r.citation.turn,
            summary: r.decision.summary.as_str(),
            rationale: r.decision.rationale.as_str(),
            alternatives: &r.decision.alternatives,
            source_snippet: r.source_snippet.as_deref(),
            project: r.project.as_deref(),
            started_at: r.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        mode: "llm",
        decisions,
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &payload)?;
    writeln!(stdout)
}

struct DecisionRow {
    citation: aghist::model::CitationRef,
    candidate: aghist::decisions::DecisionCandidate,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

fn print_decisions_table(rows: &[DecisionRow]) {
    println!(
        "{:<6}  {:<36}  {:<24}  SNIPPET",
        "SCORE", "REF", "MARKERS"
    );
    for row in rows {
        let r = row.citation.to_string();
        let r = truncate(&r, 36);
        let markers = row.candidate.markers.join(",");
        let markers = truncate(&markers, 24);
        let snippet = truncate(&row.candidate.snippet, 80);
        println!(
            "{:<6.2}  {:<36}  {:<24}  {}",
            row.candidate.score, r, markers, snippet
        );
    }
}

fn print_decisions_json(rows: &[DecisionRow]) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: aghist::model::Provider,
        session_id: &'a str,
        turn: u32,
        role: aghist::model::Role,
        score: f32,
        markers: &'a [String],
        snippet: &'a str,
        project: Option<&'a str>,
        timestamp: chrono::DateTime<chrono::Utc>,
        started_at: chrono::DateTime<chrono::Utc>,
    }

    #[derive(serde::Serialize)]
    struct Payload<'a> {
        decisions: Vec<JsonRow<'a>>,
        count: usize,
    }

    let decisions: Vec<JsonRow> = rows
        .iter()
        .map(|r| JsonRow {
            reference: r.citation.to_string(),
            provider: r.citation.provider,
            session_id: r.citation.session_id.0.as_str(),
            turn: r.citation.turn,
            role: r.candidate.role,
            score: r.candidate.score,
            markers: &r.candidate.markers,
            snippet: r.candidate.snippet.as_str(),
            project: r.project.as_deref(),
            timestamp: r.candidate.timestamp,
            started_at: r.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        decisions,
    };
    serde_json::to_writer(io::stdout().lock(), &payload).map_err(std::io::Error::other)?;
    println!();
    Ok(())
}
