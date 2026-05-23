use std::collections::BTreeMap;

use crate::model::{Provider, Session, TokenUsage};

use super::types::{GroupBy, UsageReport, UsageRow, UsageTotals};
use super::{pricing_for, round_cents_4};

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
