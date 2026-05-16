use aghist::todos::TodoKind;
use clap::Args;

use super::super::resolvers::parse_todo_kind;

#[derive(Args)]
pub(crate) struct TrackCommand {
    /// Free-text topic to track (e.g. "auth middleware", "BM25 scoring").
    #[arg(value_name = "TOPIC")]
    pub(crate) topic: String,

    /// Maximum sessions to scan for the topic (0 = no limit, default 50).
    #[arg(long, short = 'n', default_value_t = 50)]
    pub(crate) limit: usize,

    /// Force JSON output (default: table on TTY, JSON on pipe).
    #[arg(long)]
    pub(crate) json: bool,

    /// Override the LLM model id (default: from `AGHIST_LLM_MODEL` or claude-haiku-4-5).
    #[arg(long, value_name = "MODEL")]
    pub(crate) llm_model: Option<String>,
}

#[derive(Args)]
pub(crate) struct DecisionsCommand {
    /// Restrict to a single session by id, unique id prefix, or full
    /// citation ref `<provider>/<session-id>#<turn>` (turn ignored).
    #[arg(long, short = 's', value_name = "SESSION_OR_REF")]
    pub(crate) session: Option<String>,

    /// Drop sentences whose score is below this threshold.
    /// Default 3.0 keeps explicit decisions and pairs of soft markers.
    #[arg(long, default_value_t = aghist::decisions::DEFAULT_THRESHOLD, value_name = "FLOAT")]
    pub(crate) threshold: f32,

    /// Maximum number of candidates to return across all sessions,
    /// after sorting by score descending (heuristic) or by recency (--llm).
    #[arg(long, short = 'n', default_value_t = 50)]
    pub(crate) limit: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,

    /// Route heuristic candidates through an LLM for structured extraction.
    /// Requires `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
    #[arg(long)]
    pub(crate) llm: bool,

    /// Override the LLM model id (default: claude-haiku-4-5-20251001
    /// or `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
    #[arg(long, value_name = "MODEL")]
    pub(crate) llm_model: Option<String>,
}

#[derive(Args)]
pub(crate) struct TodosCommand {
    /// Restrict to one or more kinds. Repeat the flag, or comma-separate.
    /// Valid: `todo`, `follow-up`, `come-back-to`, `we-should`, `bd-ref`.
    #[arg(long, value_delimiter = ',', value_parser = parse_todo_kind, value_name = "KIND")]
    pub(crate) kind: Vec<TodoKind>,

    /// Maximum number of candidates to emit (0 = no limit).
    #[arg(long, short = 'n', default_value_t = 200)]
    pub(crate) limit: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,

    /// Route heuristic candidates through an LLM for structured extraction
    /// (`description` / `target_session` / `status_inferred`). Requires
    /// `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`).
    #[arg(long)]
    pub(crate) llm: bool,

    /// Override the LLM model id (default: claude-haiku-4-5-20251001 or
    /// `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
    #[arg(long, value_name = "MODEL")]
    pub(crate) llm_model: Option<String>,
}

#[derive(Args)]
pub(crate) struct ThreadsCommand {
    /// Cluster gap in hours. Sessions in the same project within this gap
    /// merge into one thread; longer gaps split. Ignored with `--llm`.
    #[arg(long, default_value_t = aghist::threads::DEFAULT_GAP_HOURS, value_name = "HOURS")]
    pub(crate) gap_hours: i64,

    /// Drop threads with fewer than this many sessions. Ignored with `--llm`.
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub(crate) min_sessions: usize,

    /// Maximum number of threads to emit (0 = no limit).
    #[arg(long, short = 'n', default_value_t = 50)]
    pub(crate) limit: usize,

    /// Force JSON output (default: JSON on pipe, table on TTY).
    #[arg(long)]
    pub(crate) json: bool,

    /// Route session digests through an LLM for semantic topic clustering
    /// across project boundaries. Requires `ANTHROPIC_API_KEY` (or
    /// `AGHIST_LLM_API_KEY`).
    #[arg(long)]
    pub(crate) llm: bool,

    /// Override the LLM model id (default: claude-haiku-4-5-20251001 or
    /// `AGHIST_LLM_MODEL`). Only meaningful with `--llm`.
    #[arg(long, value_name = "MODEL")]
    pub(crate) llm_model: Option<String>,

    /// Cap on session digests sent to the LLM (most recent kept). One
    /// digest is ~150 bytes, so 200 input is roughly 7.5K tokens per call. Only
    /// meaningful with `--llm`.
    #[arg(long, default_value_t = 200, value_name = "N")]
    pub(crate) llm_max_sessions: usize,
}
