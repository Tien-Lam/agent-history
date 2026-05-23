use crate::model::TokenUsage;

mod table;

use table::PRICING_TABLE;

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
    /// (we don't extrapolate from base rates; better to under-report
    /// than to invent numbers).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // u64 -> f64: token counts << 2^52
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

/// Look up a model's pricing by longest-prefix match against the
/// curated table. Returns `None` for any model not in the table;
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
