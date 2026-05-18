use std::collections::HashMap;
use std::time::Duration;

use crate::rankers::{RankedHit, TOP_K};

pub(crate) const MIN_LEXICAL_RECALL: f32 = 0.85;
pub(crate) const MIN_HYBRID_RECALL: f32 = 0.85;
pub(crate) const MIN_HYBRID_MRR: f32 = 0.80;
pub(crate) const MAX_INDEX_BUILD_TIME: Duration = Duration::from_secs(10);
pub(crate) const MAX_QUERY_P95: Duration = Duration::from_millis(750);

/// Rank of `expected` within `hits`, or `None` if not in the top `TOP_K`.
pub(crate) fn rank_of(hits: &[RankedHit], expected: &str) -> Option<usize> {
    hits.iter()
        .take(TOP_K)
        .position(|h| h.session_id == expected)
        .map(|i| i + 1)
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Metrics {
    n: usize,
    hits_at_k: usize,
    reciprocal_rank_sum: f32,
}

impl Metrics {
    pub(crate) fn record(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            self.hits_at_k += 1;
            self.reciprocal_rank_sum += 1.0 / r as f32;
        }
    }

    pub(crate) fn recall(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.hits_at_k as f32 / self.n as f32
        }
    }

    pub(crate) fn mrr(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.reciprocal_rank_sum / self.n as f32
        }
    }
}

/// One row of the per-strategy report: overall + per-tag breakdowns.
#[derive(Debug, Default, Clone)]
pub(crate) struct StrategyReport {
    pub(crate) strategy: String,
    pub(crate) overall: Metrics,
    pub(crate) per_tag: HashMap<String, Metrics>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BenchTimings {
    pub(crate) index_build: Duration,
    pub(crate) query_p95: Duration,
}

pub(crate) fn percentile_duration(
    mut durations: Vec<Duration>,
    percentile_percent: usize,
) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    durations.sort_unstable();
    let max_idx = durations.len() - 1;
    let idx = max_idx.saturating_mul(percentile_percent).div_ceil(100);
    durations[idx.min(max_idx)]
}
