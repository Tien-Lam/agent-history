//! Per-provider format-fidelity diagnostic.
//!
//! Runs a provider through `discover_sessions` + `load_messages` and counts
//! [`ContentBlock`] kinds and tool-call/tool-result pairing fidelity. The
//! resulting [`ProviderDiagnostic`] is a stable serialisable record that
//! both `tests/provider_format_diagnostic.rs` (asserting on fixtures) and
//! `aghist health` (sampling real user data) consume.

use serde::Serialize;

use crate::model::{ContentBlock, Message};
use crate::provider::{HistoryProvider, ProviderError, ProviderParseStats};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct BlockCounts {
    pub text: usize,
    pub code_block: usize,
    pub tool_use: usize,
    pub tool_result: usize,
    pub thinking: usize,
    pub error: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ToolCallFidelity {
    /// Total `ContentBlock::ToolUse` blocks observed.
    pub tool_calls: usize,
    /// Total `ContentBlock::ToolResult` blocks observed.
    pub tool_results: usize,
    /// Tool calls whose `id` was matched by at least one tool result's
    /// `tool_call_id` within the same provider's loaded messages.
    pub paired: usize,
    /// Tool calls with no matching tool result.
    pub unpaired_calls: usize,
    /// Tool results with no matching tool call.
    pub orphan_results: usize,
    /// Tool calls with an empty `name`.
    pub empty_names: usize,
    /// Tool calls with an empty `id`.
    pub empty_call_ids: usize,
    /// Tool results with an empty `tool_call_id`.
    pub empty_result_ids: usize,
    /// Tool calls whose `arguments` is non-empty but does not parse as JSON.
    /// Empty arguments are not counted (some providers omit args entirely).
    pub invalid_json_args: usize,
    pub success_results: usize,
    pub failure_results: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDiagnostic {
    /// Caller-supplied label (e.g. `"claude"`, `"codex_v2"`). Distinct from
    /// `provider` so the same provider impl can be diagnosed against
    /// different fixture roots.
    pub label: String,
    /// Slug from [`HistoryProvider::provider`] (e.g. `"claude-code"`).
    pub provider: String,
    pub session_count: usize,
    pub message_count: usize,
    pub parse: ProviderParseStats,
    pub blocks: BlockCounts,
    pub tool_call_fidelity: ToolCallFidelity,
}

/// Aggregate `[ContentBlock]` and tool-call statistics for a flat slice of
/// messages. Pairing is computed within this slice only — callers wanting
/// per-session pairing should call this per session.
#[must_use]
pub fn analyze_messages(messages: &[Message]) -> (BlockCounts, ToolCallFidelity) {
    let mut blocks = BlockCounts::default();
    let mut fidelity = ToolCallFidelity::default();

    let mut call_ids: Vec<String> = Vec::new();
    let mut result_ids: Vec<String> = Vec::new();

    for msg in messages {
        for block in &msg.content {
            blocks.total += 1;
            match block {
                ContentBlock::Text(_) => blocks.text += 1,
                ContentBlock::CodeBlock { .. } => blocks.code_block += 1,
                ContentBlock::Thinking(_) => blocks.thinking += 1,
                ContentBlock::Error(_) => blocks.error += 1,
                ContentBlock::ToolUse(tc) => {
                    blocks.tool_use += 1;
                    fidelity.tool_calls += 1;
                    if tc.name.is_empty() {
                        fidelity.empty_names += 1;
                    }
                    if tc.id.is_empty() {
                        fidelity.empty_call_ids += 1;
                    } else {
                        call_ids.push(tc.id.clone());
                    }
                    if !tc.arguments.is_empty()
                        && serde_json::from_str::<serde_json::Value>(&tc.arguments).is_err()
                    {
                        fidelity.invalid_json_args += 1;
                    }
                }
                ContentBlock::ToolResult(tr) => {
                    blocks.tool_result += 1;
                    fidelity.tool_results += 1;
                    if tr.success {
                        fidelity.success_results += 1;
                    } else {
                        fidelity.failure_results += 1;
                    }
                    if tr.tool_call_id.is_empty() {
                        fidelity.empty_result_ids += 1;
                    } else {
                        result_ids.push(tr.tool_call_id.clone());
                    }
                }
            }
        }
    }

    let result_set: std::collections::HashSet<&str> =
        result_ids.iter().map(String::as_str).collect();
    let call_set: std::collections::HashSet<&str> = call_ids.iter().map(String::as_str).collect();

    for id in &call_ids {
        if result_set.contains(id.as_str()) {
            fidelity.paired += 1;
        } else {
            fidelity.unpaired_calls += 1;
        }
    }
    for id in &result_ids {
        if !call_set.contains(id.as_str()) {
            fidelity.orphan_results += 1;
        }
    }

    (blocks, fidelity)
}

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

#[cfg(test)]
mod tests;
