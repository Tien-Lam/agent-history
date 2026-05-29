//! Aggregate token usage and (when pricing is known) USD cost across
//! sessions. Powers the `aghist usage` subcommand.
//!
//! ## What's here
//!
//! - [`ModelPricing`]: per-million-token rates for one model.
//! - [`pricing_for`]: map a model id (`claude-sonnet-4-5-20250929`) to its
//!   pricing. Matches by longest known prefix so dated variants resolve
//!   to the same family rate.
//! - [`aggregate`]: walk a slice of [`crate::model::Session`]s and produce one
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

mod aggregate;
mod pricing;
mod types;

pub use aggregate::aggregate;
pub use pricing::{pricing_for, ModelPricing};
pub use types::{GroupBy, UsageReport, UsageRow, UsageTotals};

/// Round a USD figure to 4 decimal places (1/100th of a cent). Cost
/// totals over many sessions accumulate sub-cent fractions — rounding
/// at emit-time keeps the JSON readable without losing precision a
/// user would notice.
pub(crate) fn round_cents_4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests;
