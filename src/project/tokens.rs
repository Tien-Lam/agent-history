use serde::Serialize;

use crate::model::{Message, Session};
use crate::usage::{pricing_for, round_cents_4};

/// Aggregated token totals across the matched sessions.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
    /// USD cost across the project, or `None` when any constituent session
    /// uses an unpriced model. Same convention as `aghist usage`.
    pub cost_usd: Option<f64>,
}

/// Aggregate token totals across the given session bundles.
///
/// Sessions without a usage block contribute zero tokens but don't null
/// out cost. Sessions whose `model` is unknown to the pricing table null
/// the cost field, matching the `aghist usage`/`aghist project` convention.
#[must_use]
pub fn aggregate_tokens(sessions: &[(Session, Vec<Message>)]) -> ProjectTokens {
    let mut t = ProjectTokens::default();
    let mut cost = 0.0f64;
    let mut has_unpriced = false;
    for (s, _) in sessions {
        let Some(usage) = s.token_usage.as_ref() else {
            // Missing usage is not the same as an unpriced model.
            continue;
        };
        t.input_tokens = t.input_tokens.saturating_add(usage.input_tokens);
        t.output_tokens = t.output_tokens.saturating_add(usage.output_tokens);
        t.cache_read_tokens = t
            .cache_read_tokens
            .saturating_add(usage.cache_read_tokens.unwrap_or(0));
        t.cache_write_tokens = t
            .cache_write_tokens
            .saturating_add(usage.cache_write_tokens.unwrap_or(0));
        match s.model.as_deref().and_then(pricing_for) {
            Some(p) => cost += p.cost_usd(usage),
            None => has_unpriced = true,
        }
    }
    t.total_tokens = t
        .input_tokens
        .saturating_add(t.output_tokens)
        .saturating_add(t.cache_read_tokens)
        .saturating_add(t.cache_write_tokens);
    t.cost_usd = if has_unpriced {
        None
    } else {
        Some(round_cents_4(cost))
    };
    t
}
