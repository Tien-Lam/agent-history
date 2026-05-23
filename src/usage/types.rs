use serde::Serialize;

/// How rows in a usage report are bucketed. The aggregator sums every
/// session's tokens (and cost, when known) into the bucket whose key it
/// produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// One row per distinct `model` value (or `(unknown)` for sessions
    /// without a model). Default - most useful for cost analysis.
    Model,
    /// One row per provider slug. Useful for "how much have I spent
    /// across Claude Code vs Codex CLI" comparisons.
    Provider,
    /// One row per `project_name` (or `(unknown)` for sessions without).
    Project,
}

impl GroupBy {
    /// Stable label used in JSON output and `--by` parsing.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::Model => "model",
            GroupBy::Provider => "provider",
            GroupBy::Project => "project",
        }
    }

    /// Parse `--by` value. Returns `Err(input)` on unknown values so the
    /// caller can surface a usage error with the offending string.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "model" => Ok(GroupBy::Model),
            "provider" => Ok(GroupBy::Provider),
            "project" => Ok(GroupBy::Project),
            other => Err(other.to_string()),
        }
    }
}

/// One aggregated row in a usage report. `cost_usd` is `None` when at
/// least one session in the bucket has a model whose pricing we don't
/// know - partial costs would be misleading, so the bucket reports
/// no cost rather than a low-balled subtotal.
#[derive(Debug, Clone, Serialize)]
pub struct UsageRow {
    /// The bucket key: model id, provider slug, or project name. Empty
    /// values are normalized to `"(unknown)"` so the column always has
    /// content.
    pub key: String,
    pub session_count: usize,
    pub message_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
    /// USD cost across the bucket, or `None` when any constituent
    /// session uses an unpriced model.
    pub cost_usd: Option<f64>,
}

/// Whole-report total. Mirrors [`UsageRow`] but spans every input
/// session, regardless of group. `cost_usd` is `None` if *any* session
/// hit an unpriced model.
#[derive(Debug, Clone, Serialize)]
pub struct UsageTotals {
    pub session_count: usize,
    pub message_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
}

/// Result of `aggregate`: ranked rows plus the overall totals.
#[derive(Debug, Clone, Serialize)]
pub struct UsageReport {
    pub rows: Vec<UsageRow>,
    pub totals: UsageTotals,
}
