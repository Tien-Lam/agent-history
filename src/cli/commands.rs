use clap::Subcommand;

mod analysis;
mod lookup;
mod metadata;
mod reports;

use analysis::{DecisionsCommand, ThreadsCommand, TodosCommand, TrackCommand};
use lookup::{DiffCommand, ExportCommand, IndexCommand, SearchCommand, ShowCommand};
pub(crate) use metadata::{NoteCommand, SourcesCommand, TagCommand};
use reports::{ProjectCommand, ReportCommand, UsageCommand};

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Export a session to Markdown, JSON, or HTML
    Export(ExportCommand),
    /// Build or refresh the search index. Idempotent and delta-aware.
    ///
    /// Skips sessions whose source files are unchanged since the last run,
    /// re-indexes those that have changed, and indexes any new sessions.
    /// Always exits with status 0 on success and prints a JSON summary
    /// of `added` / `updated` / `unchanged` counts to stdout.
    Index(IndexCommand),
    /// Search indexed sessions for a query
    Search(SearchCommand),
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
    Show(ShowCommand),
    /// Compare two sessions turn-by-turn in diff-hunk style.
    ///
    /// Computes the longest-common-subsequence of the two sessions' messages
    /// (keyed by role + first-64-chars of content) and emits hunks of
    /// diverging turns, with 2 lines of shared context around each hunk.
    /// Useful for "compare yesterday's debug session with today's working one".
    ///
    /// Session refs: `<provider>/<session-id>` (no turn suffix).
    Diff(DiffCommand),
    /// Track how a topic evolved across sessions (LLM-required).
    ///
    /// Finds sessions that mention the topic by keyword, extracts relevant
    /// excerpts, and asks the LLM what *changed* about the topic across them.
    /// Output is a chronological timeline: `{session_ref, date, event, direction}`.
    /// Directions: `introduced`, `revised`, `confirmed`, `dropped`.
    ///
    /// Requires `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
    Track(TrackCommand),
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
    Decisions(DecisionsCommand),
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
    Todos(TodosCommand),
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
    Threads(ThreadsCommand),
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
    Usage(UsageCommand),
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
    Project(ProjectCommand),
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
    Report(ReportCommand),
    /// Update a self-managed release binary to the latest GitHub release
    Update,
    /// Remove a self-managed release binary and data
    Uninstall,
}
