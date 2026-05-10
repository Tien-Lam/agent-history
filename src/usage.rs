//! Aggregate token usage and (when pricing is known) USD cost across
//! sessions. Powers the `aghist usage` subcommand.
//!
//! ## What's here
//!
//! - [`ModelPricing`]: per-million-token rates for one model.
//! - [`pricing_for`]: map a model id (`claude-sonnet-4-5-20250929`) to its
//!   pricing. Matches by longest known prefix so dated variants resolve
//!   to the same family rate.
//! - [`aggregate`]: walk a slice of [`Session`]s and produce one
//!   [`UsageRow`] per group key, plus an overall total.
//!
//! ## What's *not* here
//!
//! Cost is an approximation: provider snapshots store totals, not the
//! per-request breakdown the model APIs return. We trust whatever
//! `Session::token_usage` reports and apply rates uniformly. Sessions
//! without pricing contribute to token totals but their cost is
//! reported as `null`. Cache reads/writes are folded in when the
//! provider populated them; otherwise they're zero.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::{Provider, Session, TokenUsage};

/// Per-1M-token rates in USD. Cache rates are optional because not every
/// provider/model pair publishes them; missing rates contribute zero
/// rather than silently inflating cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: Option<f64>,
    pub cache_write_per_mtok: Option<f64>,
}

impl ModelPricing {
    /// Compute USD cost for the given token counts. Cache reads/writes
    /// are skipped silently when the model has no published cache rate
    /// (we don't extrapolate from base rates — better to under-report
    /// than to invent numbers).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // u64 → f64: token counts << 2^52
    pub fn cost_usd(&self, usage: &TokenUsage) -> f64 {
        let input = (usage.input_tokens as f64 / 1_000_000.0) * self.input_per_mtok;
        let output = (usage.output_tokens as f64 / 1_000_000.0) * self.output_per_mtok;
        let cache_read = match (usage.cache_read_tokens, self.cache_read_per_mtok) {
            (Some(tok), Some(rate)) => (tok as f64 / 1_000_000.0) * rate,
            _ => 0.0,
        };
        let cache_write = match (usage.cache_write_tokens, self.cache_write_per_mtok) {
            (Some(tok), Some(rate)) => (tok as f64 / 1_000_000.0) * rate,
            _ => 0.0,
        };
        input + output + cache_read + cache_write
    }
}

/// Hand-curated price list. Keys are *prefixes*: lookup matches the
/// longest entry that is a prefix of the model id, so undated and dated
/// model ids (`claude-sonnet-4-5` vs `claude-sonnet-4-5-20250929`)
/// resolve to the same rate without per-snapshot maintenance.
///
/// Sources: Anthropic public pricing page (claude-*), `OpenAI` pricing
/// page (gpt-*, o-series), Google Vertex/AI Studio (gemini-*) as of
/// the snapshot below. These are *list* prices; volume discounts and
/// batch-API rates are out of scope.
///
/// **Keep this list small and conservative.** Unknown models report
/// `cost_usd: null` rather than guessing — silent guesses become silent
/// bugs in finance reports.
const PRICING_TABLE: &[(&str, ModelPricing)] = &[
    // Claude 4.x family (cache rates: write = base * 1.25, read = base * 0.10).
    (
        "claude-opus-4",
        ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.5),
            cache_write_per_mtok: Some(18.75),
        },
    ),
    (
        "claude-sonnet-4",
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
    ),
    (
        "claude-haiku-4",
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cache_read_per_mtok: Some(0.1),
            cache_write_per_mtok: Some(1.25),
        },
    ),
    // Claude 3.x family.
    (
        "claude-3-5-sonnet",
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
    ),
    (
        "claude-3-5-haiku",
        ModelPricing {
            input_per_mtok: 0.8,
            output_per_mtok: 4.0,
            cache_read_per_mtok: Some(0.08),
            cache_write_per_mtok: Some(1.0),
        },
    ),
    (
        "claude-3-opus",
        ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.5),
            cache_write_per_mtok: Some(18.75),
        },
    ),
    (
        "claude-3-haiku",
        ModelPricing {
            input_per_mtok: 0.25,
            output_per_mtok: 1.25,
            cache_read_per_mtok: Some(0.03),
            cache_write_per_mtok: Some(0.3),
        },
    ),
    // GPT-4o / GPT-4 family. Cache read is OpenAI's "cached input" rate.
    (
        "gpt-4o-mini",
        ModelPricing {
            input_per_mtok: 0.15,
            output_per_mtok: 0.6,
            cache_read_per_mtok: Some(0.075),
            cache_write_per_mtok: None,
        },
    ),
    (
        "gpt-4o",
        ModelPricing {
            input_per_mtok: 2.5,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(1.25),
            cache_write_per_mtok: None,
        },
    ),
    (
        "gpt-4-turbo",
        ModelPricing {
            input_per_mtok: 10.0,
            output_per_mtok: 30.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    // Gemini 1.5 / 2.x family.
    (
        "gemini-2.5-pro",
        ModelPricing {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-2.5-flash",
        ModelPricing {
            input_per_mtok: 0.3,
            output_per_mtok: 2.5,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-1.5-pro",
        ModelPricing {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-1.5-flash",
        ModelPricing {
            input_per_mtok: 0.075,
            output_per_mtok: 0.3,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
];

/// Look up a model's pricing by longest-prefix match against the
/// curated table. Returns `None` for any model not in the table —
/// callers report cost as `null` for those rows.
#[must_use]
pub fn pricing_for(model: &str) -> Option<ModelPricing> {
    let mut best: Option<(usize, ModelPricing)> = None;
    for (prefix, pricing) in PRICING_TABLE {
        if model.starts_with(prefix) && best.is_none_or(|(len, _)| prefix.len() > len) {
            best = Some((prefix.len(), *pricing));
        }
    }
    best.map(|(_, p)| p)
}

/// How rows in a usage report are bucketed. The aggregator sums every
/// session's tokens (and cost, when known) into the bucket whose key it
/// produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// One row per distinct `model` value (or `(unknown)` for sessions
    /// without a model). Default — most useful for cost analysis.
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
/// know — partial costs would be misleading, so the bucket reports
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

/// Round a USD figure to 4 decimal places (1/100th of a cent). Cost
/// totals over many sessions accumulate sub-cent fractions — rounding
/// at emit-time keeps the JSON readable without losing precision a
/// user would notice.
fn round_cents_4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Aggregate token usage and cost across `sessions`, grouped by the
/// chosen dimension. Rows are sorted by `total_tokens` descending so
/// the heaviest buckets surface first, with the bucket key as a
/// deterministic tiebreaker.
///
/// Sessions without `token_usage` are counted toward `session_count`
/// and `message_count` (so totals match `--list`) but contribute zero
/// tokens — they're not erased, just have nothing to add.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionId;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn mk_session(
        id: &str,
        provider: Provider,
        model: Option<&str>,
        project: Option<&str>,
        usage: Option<TokenUsage>,
    ) -> Session {
        Session {
            id: SessionId(id.to_string()),
            provider,
            project_path: project.map(PathBuf::from),
            project_name: project.map(str::to_string),
            git_branch: None,
            started_at: ts(0),
            ended_at: None,
            summary: None,
            model: model.map(str::to_string),
            token_usage: usage,
            message_count: 1,
            source_path: PathBuf::from(format!("/tmp/{id}")),
        }
    }

    #[test]
    fn pricing_for_known_prefix_returns_rate() {
        let p = pricing_for("claude-sonnet-4-5-20250929").unwrap();
        assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_mtok - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pricing_for_longer_prefix_wins() {
        // Both "claude-3-5-sonnet" and "claude-3-haiku" share "claude-3-",
        // but neither is a prefix of "claude-3-5-sonnet-20240620". The
        // longest matching prefix should be the 3-5-sonnet entry.
        let p = pricing_for("claude-3-5-sonnet-20240620").unwrap();
        assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pricing_for_unknown_returns_none() {
        assert!(pricing_for("totally-imaginary-model").is_none());
        assert!(pricing_for("").is_none());
    }

    #[test]
    fn cost_usd_includes_input_output_and_cache() {
        let p = pricing_for("claude-sonnet-4-5").unwrap();
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: Some(1_000_000),
            cache_write_tokens: Some(1_000_000),
        };
        // 3 + 15 + 0.3 + 3.75 = 22.05
        let cost = p.cost_usd(&usage);
        assert!((cost - 22.05).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn cost_usd_skips_cache_when_no_rate_available() {
        let p = pricing_for("gpt-4-turbo").unwrap();
        let usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: Some(1_000_000),
            cache_write_tokens: Some(1_000_000),
        };
        // GPT-4 Turbo entry has no cache rates, so the cache tokens
        // contribute zero rather than being valued at the input rate.
        assert!(p.cost_usd(&usage).abs() < f64::EPSILON);
    }

    #[test]
    fn aggregate_groups_by_model_and_sums_tokens() {
        let usage_a = TokenUsage {
            input_tokens: 1_000,
            output_tokens: 500,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let usage_b = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![
            mk_session(
                "a",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                Some("foo"),
                Some(usage_a),
            ),
            mk_session(
                "b",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                Some("foo"),
                Some(usage_b),
            ),
        ];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows.len(), 1);
        let row = &report.rows[0];
        assert_eq!(row.key, "claude-sonnet-4-5");
        assert_eq!(row.session_count, 2);
        assert_eq!(row.input_tokens, 1_200);
        assert_eq!(row.output_tokens, 600);
        assert_eq!(row.total_tokens, 1_800);
        // 0.0012 * 3 + 0.0006 * 15 = 0.0036 + 0.009 = 0.0126
        assert!((row.cost_usd.unwrap() - 0.0126).abs() < 1e-9);
        assert_eq!(report.totals.session_count, 2);
        assert!((report.totals.cost_usd.unwrap() - 0.0126).abs() < 1e-9);
    }

    #[test]
    fn aggregate_unknown_model_keeps_tokens_drops_cost() {
        let usage = TokenUsage {
            input_tokens: 1_000,
            output_tokens: 500,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![mk_session(
            "a",
            Provider::ClaudeCode,
            Some("future-model-7"),
            None,
            Some(usage),
        )];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].input_tokens, 1_000);
        assert!(report.rows[0].cost_usd.is_none());
        assert!(report.totals.cost_usd.is_none());
    }

    #[test]
    fn aggregate_priced_plus_unpriced_drops_cost_for_overall_only() {
        // Bucket A is fully priced; bucket B is unpriced. Each row's
        // own cost stands; only the overall total goes to None.
        let usage = TokenUsage {
            input_tokens: 1_000,
            output_tokens: 500,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![
            mk_session(
                "a",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                None,
                Some(usage.clone()),
            ),
            mk_session(
                "b",
                Provider::CodexCli,
                Some("future-model-7"),
                None,
                Some(usage),
            ),
        ];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows.len(), 2);
        let priced = report
            .rows
            .iter()
            .find(|r| r.key == "claude-sonnet-4-5")
            .unwrap();
        assert!(priced.cost_usd.is_some());
        let unpriced = report
            .rows
            .iter()
            .find(|r| r.key == "future-model-7")
            .unwrap();
        assert!(unpriced.cost_usd.is_none());
        assert!(report.totals.cost_usd.is_none());
    }

    #[test]
    fn aggregate_missing_model_uses_unknown_bucket() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![mk_session(
            "a",
            Provider::ClaudeCode,
            None,
            None,
            Some(usage),
        )];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].key, "(unknown)");
        assert!(report.rows[0].cost_usd.is_none());
    }

    #[test]
    fn aggregate_by_provider_groups_across_models() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![
            mk_session(
                "a",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                None,
                Some(usage.clone()),
            ),
            mk_session(
                "b",
                Provider::ClaudeCode,
                Some("claude-haiku-4-5"),
                None,
                Some(usage),
            ),
        ];
        let report = aggregate(&sessions, GroupBy::Provider);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].key, "claude-code");
        assert_eq!(report.rows[0].session_count, 2);
    }

    #[test]
    fn aggregate_by_project_normalizes_missing_to_unknown() {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![
            mk_session(
                "a",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                Some("aghist"),
                Some(usage.clone()),
            ),
            mk_session(
                "b",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                None,
                Some(usage),
            ),
        ];
        let report = aggregate(&sessions, GroupBy::Project);
        let keys: Vec<&str> = report.rows.iter().map(|r| r.key.as_str()).collect();
        assert!(keys.contains(&"aghist"));
        assert!(keys.contains(&"(unknown)"));
    }

    #[test]
    fn aggregate_sessions_without_usage_count_but_dont_inflate_tokens() {
        let sessions = vec![mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            None,
            None,
        )];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].session_count, 1);
        assert_eq!(report.rows[0].input_tokens, 0);
        assert_eq!(report.rows[0].output_tokens, 0);
        assert_eq!(report.rows[0].total_tokens, 0);
        assert_eq!(report.rows[0].cost_usd, Some(0.0));
    }

    #[test]
    fn rows_sorted_by_total_tokens_descending() {
        let big = TokenUsage {
            input_tokens: 10_000,
            output_tokens: 1_000,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let small = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let sessions = vec![
            mk_session(
                "a",
                Provider::ClaudeCode,
                Some("claude-haiku-4-5"),
                None,
                Some(small),
            ),
            mk_session(
                "b",
                Provider::ClaudeCode,
                Some("claude-sonnet-4-5"),
                None,
                Some(big),
            ),
        ];
        let report = aggregate(&sessions, GroupBy::Model);
        assert_eq!(report.rows[0].key, "claude-sonnet-4-5");
        assert_eq!(report.rows[1].key, "claude-haiku-4-5");
    }

    #[test]
    fn group_by_parse_round_trips() {
        for s in ["model", "provider", "project"] {
            let g = GroupBy::parse(s).unwrap();
            assert_eq!(g.as_str(), s);
        }
        assert!(GroupBy::parse("session").is_err());
    }
}
