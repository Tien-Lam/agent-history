//! Per-provider format-fidelity diagnostic.
//!
//! Runs a provider through `discover_sessions` + `load_messages` and counts
//! [`crate::model::ContentBlock`] kinds and tool-call/tool-result pairing
//! fidelity. The resulting [`ProviderDiagnostic`] is a stable serialisable
//! record that both `tests/provider_format_diagnostic.rs` (asserting on
//! fixtures) and `aghist health` (sampling real user data) consume.

mod messages;
mod provider;
mod types;

pub use messages::analyze_messages;
pub use provider::analyze_provider;
pub use types::{BlockCounts, ProviderDiagnostic, ToolCallFidelity};

#[cfg(test)]
mod tests;
