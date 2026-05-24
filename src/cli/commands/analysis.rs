use aghist::todos::TodoKind;
use clap::Args;

use super::super::resolvers::parse_todo_kind;

use aghist::schema_fragments::{
    ANALYSIS_DECISIONS_LIMIT_DEFAULT, ANALYSIS_LIMIT_MAX, ANALYSIS_THREADS_LIMIT_DEFAULT,
    ANALYSIS_THREADS_LLM_MAX_SESSIONS_DEFAULT, ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX,
    ANALYSIS_TODOS_LIMIT_DEFAULT, ANALYSIS_TRACK_LIMIT_DEFAULT, LLM_MODEL_MAX_BYTES,
};

#[derive(Args)]
pub(crate) struct TrackCommand {
    /// Free-text topic to track (e.g. "auth middleware", "BM25 scoring").
    #[arg(value_name = "TOPIC")]
    pub(crate) topic: String,

    /// Maximum matching sessions to send to the LLM.
    #[arg(long, short = 'n', default_value_t = ANALYSIS_TRACK_LIMIT_DEFAULT, value_parser = parse_track_limit)]
    pub(crate) limit: usize,

    /// Force JSON output (default: table on TTY, JSON on pipe).
    #[arg(long)]
    pub(crate) json: bool,

    /// Override the LLM model id (default: from `AGHIST_LLM_MODEL` or claude-haiku-4-5-20251001).
    #[arg(long, value_name = "MODEL", value_parser = parse_llm_model)]
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
    #[arg(
        long,
        default_value_t = aghist::decisions::DEFAULT_THRESHOLD,
        value_name = "FLOAT",
        value_parser = parse_decision_threshold,
        allow_hyphen_values = true
    )]
    pub(crate) threshold: f32,

    /// Maximum number of candidates to return across all sessions,
    /// after sorting by score descending (heuristic) or by recency (--llm).
    #[arg(long, short = 'n', default_value_t = ANALYSIS_DECISIONS_LIMIT_DEFAULT, value_parser = parse_decisions_limit)]
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
    #[arg(long, value_name = "MODEL", value_parser = parse_llm_model)]
    pub(crate) llm_model: Option<String>,
}

#[derive(Args)]
pub(crate) struct TodosCommand {
    /// Restrict to one or more kinds. Repeat the flag, or comma-separate.
    /// Valid: `todo`, `follow-up`, `come-back-to`, `we-should`, `bd-ref`.
    #[arg(long, value_delimiter = ',', value_parser = parse_todo_kind, value_name = "KIND")]
    pub(crate) kind: Vec<TodoKind>,

    /// Maximum number of candidates to emit.
    #[arg(long, short = 'n', default_value_t = ANALYSIS_TODOS_LIMIT_DEFAULT, value_parser = parse_todos_limit)]
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
    #[arg(long, value_name = "MODEL", value_parser = parse_llm_model)]
    pub(crate) llm_model: Option<String>,
}

#[derive(Args)]
pub(crate) struct ThreadsCommand {
    /// Cluster gap in hours. Sessions in the same project within this gap
    /// merge into one thread; longer gaps split. Ignored with `--llm`.
    #[arg(
        long,
        default_value_t = aghist::threads::DEFAULT_GAP_HOURS,
        value_name = "HOURS",
        value_parser = parse_gap_hours,
        allow_hyphen_values = true
    )]
    pub(crate) gap_hours: i64,

    /// Drop threads with fewer than this many sessions. Ignored with `--llm`.
    #[arg(long, default_value_t = 1, value_name = "N", value_parser = parse_min_sessions)]
    pub(crate) min_sessions: usize,

    /// Maximum number of threads to emit.
    #[arg(long, short = 'n', default_value_t = ANALYSIS_THREADS_LIMIT_DEFAULT, value_parser = parse_threads_limit)]
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
    #[arg(long, value_name = "MODEL", value_parser = parse_llm_model)]
    pub(crate) llm_model: Option<String>,

    /// Cap on session digests sent to the LLM (most recent kept). One
    /// digest is ~150 bytes, so 200 input is roughly 7.5K tokens per call. Only
    /// meaningful with `--llm`.
    #[arg(long, default_value_t = ANALYSIS_THREADS_LLM_MAX_SESSIONS_DEFAULT, value_name = "N", value_parser = parse_threads_llm_max_sessions)]
    pub(crate) llm_max_sessions: usize,
}

fn parse_track_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_limit(raw, "track limit", ANALYSIS_LIMIT_MAX)
}

fn parse_decisions_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_limit(raw, "decisions limit", ANALYSIS_LIMIT_MAX)
}

fn parse_todos_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_limit(raw, "todos limit", ANALYSIS_LIMIT_MAX)
}

fn parse_threads_limit(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_limit(raw, "threads limit", ANALYSIS_LIMIT_MAX)
}

fn parse_threads_llm_max_sessions(raw: &str) -> Result<usize, String> {
    parse_positive_bounded_limit(
        raw,
        "threads LLM max sessions",
        ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX,
    )
}

fn parse_positive_bounded_limit(raw: &str, label: &str, max: usize) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|e| format!("invalid {label}: {e}"))?;
    if value == 0 {
        Err(format!("{label} must be at least 1"))
    } else if value > max {
        Err(format!("{label} must be at most {max}"))
    } else {
        Ok(value)
    }
}

fn parse_decision_threshold(raw: &str) -> Result<f32, String> {
    let value = raw
        .parse::<f32>()
        .map_err(|e| format!("invalid decision threshold: {e}"))?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err("decision threshold must be a finite number at least 0".to_string())
    }
}

fn parse_gap_hours(raw: &str) -> Result<i64, String> {
    let value = raw
        .parse::<i64>()
        .map_err(|e| format!("invalid gap hours: {e}"))?;
    if value < 0 {
        Err("gap hours must be at least 0".to_string())
    } else {
        Ok(value)
    }
}

fn parse_min_sessions(raw: &str) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|e| format!("invalid min sessions: {e}"))?;
    if value == 0 {
        Err("min sessions must be at least 1".to_string())
    } else {
        Ok(value)
    }
}

fn parse_llm_model(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Err("LLM model must not be blank".to_string())
    } else if trimmed.len() > LLM_MODEL_MAX_BYTES {
        Err(format!(
            "LLM model must be at most {LLM_MODEL_MAX_BYTES} bytes"
        ))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_decisions_limit, parse_llm_model, parse_threads_limit,
        parse_threads_llm_max_sessions, parse_todos_limit, parse_track_limit,
    };
    use aghist::schema_fragments::{
        ANALYSIS_LIMIT_MAX, ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX, LLM_MODEL_MAX_BYTES,
    };

    #[test]
    fn parse_llm_model_rejects_blank_values() {
        assert!(parse_llm_model("").is_err());
        assert!(parse_llm_model(" \t ").is_err());
    }

    #[test]
    fn parse_llm_model_trims_valid_values() {
        assert_eq!(parse_llm_model(" claude-haiku ").unwrap(), "claude-haiku");
    }

    #[test]
    fn parse_llm_model_rejects_values_above_max() {
        let oversized = "x".repeat(LLM_MODEL_MAX_BYTES + 1);
        assert!(parse_llm_model(&oversized)
            .unwrap_err()
            .contains("must be at most"));
    }

    #[test]
    fn analysis_limit_parsers_reject_zero() {
        for (label, result) in [
            ("track", parse_track_limit("0")),
            ("decisions", parse_decisions_limit("0")),
            ("todos", parse_todos_limit("0")),
            ("threads", parse_threads_limit("0")),
            (
                "threads LLM max sessions",
                parse_threads_llm_max_sessions("0"),
            ),
        ] {
            assert!(
                result.unwrap_err().contains("must be at least 1"),
                "{label} parser should reject zero"
            );
        }
    }

    #[test]
    fn analysis_limit_parsers_reject_values_above_max() {
        let oversized = (ANALYSIS_LIMIT_MAX + 1).to_string();
        for (label, result) in [
            ("track", parse_track_limit(&oversized)),
            ("decisions", parse_decisions_limit(&oversized)),
            ("todos", parse_todos_limit(&oversized)),
            ("threads", parse_threads_limit(&oversized)),
        ] {
            assert!(
                result.unwrap_err().contains("must be at most"),
                "{label} parser should reject oversized limits"
            );
        }

        let oversized_llm = (ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX + 1).to_string();
        assert!(parse_threads_llm_max_sessions(&oversized_llm)
            .unwrap_err()
            .contains("must be at most"));
    }
}
