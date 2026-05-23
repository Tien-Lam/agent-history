use serde::Serialize;

use crate::provider::ProviderParseStats;

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
    /// Provider slug (e.g. `"claude-code"`).
    pub provider: String,
    pub session_count: usize,
    pub message_count: usize,
    pub parse: ProviderParseStats,
    pub blocks: BlockCounts,
    pub tool_call_fidelity: ToolCallFidelity,
}
