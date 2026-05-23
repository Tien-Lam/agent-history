use crate::provider::{HistoryProvider, ProviderError, ProviderParseStats};

use super::{analyze_messages, BlockCounts, ProviderDiagnostic, ToolCallFidelity};

/// Run `discover_sessions` + `load_messages` on every session the provider
/// exposes and compute an aggregate [`ProviderDiagnostic`]. Tool-call pairing
/// is computed per session and summed (so a call in session A doesn't pair
/// with a result in session B).
///
/// `max_sessions` caps the number of sessions sampled per provider, which
/// matters for runtime callers (e.g. `aghist health`) that must not block on
/// huge real-world session stores. Pass `None` for unbounded (tests).
///
/// # Errors
///
/// Returns an error if `discover_sessions` fails. Per-session
/// `load_messages` failures are surfaced as the first error encountered;
/// successful sessions before that error contribute to the diagnostic.
pub fn analyze_provider(
    label: &str,
    provider: &dyn HistoryProvider,
    max_sessions: Option<usize>,
) -> Result<ProviderDiagnostic, ProviderError> {
    let sessions = provider.discover_sessions()?;
    let provider_slug = provider.provider().slug().to_string();
    let take = max_sessions.unwrap_or(sessions.len()).min(sessions.len());

    let mut diag = ProviderDiagnostic {
        label: label.to_string(),
        provider: provider_slug,
        session_count: sessions.len(),
        message_count: 0,
        parse: ProviderParseStats::default(),
        blocks: BlockCounts::default(),
        tool_call_fidelity: ToolCallFidelity::default(),
    };

    for session in sessions.iter().take(take) {
        let load = provider.load_messages_with_stats(session)?;
        diag.message_count += load.messages.len();
        diag.parse.merge(&load.parse_stats);
        let (blocks, fidelity) = analyze_messages(&load.messages);
        merge_block_counts(&mut diag.blocks, &blocks);
        merge_tool_fidelity(&mut diag.tool_call_fidelity, &fidelity);
    }

    Ok(diag)
}

fn merge_block_counts(into: &mut BlockCounts, from: &BlockCounts) {
    into.text += from.text;
    into.code_block += from.code_block;
    into.tool_use += from.tool_use;
    into.tool_result += from.tool_result;
    into.thinking += from.thinking;
    into.error += from.error;
    into.total += from.total;
}

fn merge_tool_fidelity(into: &mut ToolCallFidelity, from: &ToolCallFidelity) {
    into.tool_calls += from.tool_calls;
    into.tool_results += from.tool_results;
    into.paired += from.paired;
    into.unpaired_calls += from.unpaired_calls;
    into.orphan_results += from.orphan_results;
    into.empty_names += from.empty_names;
    into.empty_call_ids += from.empty_call_ids;
    into.empty_result_ids += from.empty_result_ids;
    into.invalid_json_args += from.invalid_json_args;
    into.success_results += from.success_results;
    into.failure_results += from.failure_results;
}
