use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::{Provider, Session, TokenUsage};

use super::{pricing_for, round_cents_4};

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

/// Result of [`aggregate`]: ranked rows plus the overall totals.
#[derive(Debug, Clone, Serialize)]
pub struct UsageReport {
    pub rows: Vec<UsageRow>,
    pub totals: UsageTotals,
}

#[derive(Default)]
struct BucketAcc {
    session_count: usize,
    message_count: usize,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    cost_usd: f64,
    /// Once true, the bucket reports no cost: at least one session used
    /// a model we don't have pricing for, so any partial sum we report
    /// would silently understate spend.
    has_unpriced: bool,
}

impl BucketAcc {
    fn add(&mut self, session: &Session, usage: &TokenUsage) {
        self.session_count += 1;
        self.message_count += session.message_count;
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_read_tokens += usage.cache_read_tokens.unwrap_or(0);
        self.cache_write_tokens += usage.cache_write_tokens.unwrap_or(0);
        match session.model.as_deref().and_then(pricing_for) {
            Some(p) => self.cost_usd += p.cost_usd(usage),
            None => self.has_unpriced = true,
        }
    }

    fn into_row(self, key: String) -> UsageRow {
        let total = self
            .input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens);
        UsageRow {
            key,
            session_count: self.session_count,
            message_count: self.message_count,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens,
            total_tokens: total,
            cost_usd: if self.has_unpriced {
                None
            } else {
                Some(round_cents_4(self.cost_usd))
            },
        }
    }
}

/// Aggregate token usage and cost across `sessions`, grouped by the
/// chosen dimension. Rows are sorted by `total_tokens` descending so
/// the heaviest buckets surface first, with the bucket key as a
/// deterministic tiebreaker.
///
/// Sessions without `token_usage` are counted toward `session_count`
/// and `message_count` (so totals match `--list`) but contribute zero
/// tokens - they're not erased, just have nothing to add.
#[must_use]
pub fn aggregate(sessions: &[Session], group_by: GroupBy) -> UsageReport {
    let mut buckets: BTreeMap<String, BucketAcc> = BTreeMap::new();
    let mut totals = BucketAcc::default();
    let zero = TokenUsage::default();

    for s in sessions {
        let usage = s.token_usage.as_ref().unwrap_or(&zero);
        let key = bucket_key(s, group_by);
        buckets.entry(key).or_default().add(s, usage);
        totals.add(s, usage);
    }

    let mut rows: Vec<UsageRow> = buckets
        .into_iter()
        .map(|(k, acc)| acc.into_row(k))
        .collect();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.key.cmp(&b.key))
    });

    let total_tokens = totals
        .input_tokens
        .saturating_add(totals.output_tokens)
        .saturating_add(totals.cache_read_tokens)
        .saturating_add(totals.cache_write_tokens);
    let totals = UsageTotals {
        session_count: totals.session_count,
        message_count: totals.message_count,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        cache_write_tokens: totals.cache_write_tokens,
        total_tokens,
        cost_usd: if totals.has_unpriced {
            None
        } else {
            Some(round_cents_4(totals.cost_usd))
        },
    };

    UsageReport { rows, totals }
}

fn bucket_key(s: &Session, group_by: GroupBy) -> String {
    match group_by {
        GroupBy::Model => s.model.clone().unwrap_or_else(|| "(unknown)".to_string()),
        GroupBy::Provider => provider_label(s.provider).to_string(),
        GroupBy::Project => s
            .project_name
            .clone()
            .unwrap_or_else(|| "(unknown)".to_string()),
    }
}

fn provider_label(p: Provider) -> &'static str {
    p.slug()
}
