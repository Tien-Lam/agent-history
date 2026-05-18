use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ComposerData {
    #[serde(rename = "composerId")]
    pub(crate) composer_id: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(rename = "lastUpdatedAt")]
    pub(crate) last_updated_at: Option<i64>,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<i64>,
    /// Newer Cursor builds embed bubble headers in the composer record.
    /// Each entry has `bubbleId` and a `type` (1 = user, 2 = assistant).
    #[serde(rename = "fullConversationHeadersOnly")]
    pub(crate) headers: Option<Vec<HeaderEntry>>,
    /// Some builds expose the working directory directly.
    #[serde(rename = "currentWorkspaceFolder")]
    pub(crate) workspace_folder: Option<String>,
    /// Newer schema may carry the model name on the composer record.
    pub(crate) model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HeaderEntry {
    #[serde(rename = "bubbleId")]
    pub(crate) bubble_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) bubble_type: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BubbleData {
    #[serde(rename = "type")]
    pub(crate) bubble_type: Option<u8>,
    pub(crate) text: Option<String>,
    /// Older Cursor builds put the message body under `richText` markdown.
    #[serde(rename = "richText")]
    pub(crate) rich_text: Option<String>,
    /// Inline code blocks attached to the bubble.
    #[serde(rename = "codeBlocks", default)]
    pub(crate) code_blocks: Vec<CodeBlockData>,
    /// Legacy single tool-call structure.
    #[serde(rename = "toolFormerData")]
    pub(crate) tool_former: Option<ToolFormerData>,
    /// Newer multi-tool-call structure.
    #[serde(default, rename = "toolCalls")]
    pub(crate) tool_calls: Vec<ToolCallData>,
    /// Per-bubble timestamp (newer builds).
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<i64>,
    /// Per-bubble model attribution (newer builds).
    pub(crate) model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodeBlockData {
    #[serde(rename = "languageId")]
    pub(crate) language: Option<String>,
    pub(crate) code: Option<String>,
    /// Some builds use `content` for the code body.
    pub(crate) content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolFormerData {
    #[serde(rename = "toolCallId")]
    pub(crate) tool_call_id: Option<String>,
    pub(crate) name: Option<String>,
    /// Cursor stores arguments as a JSON object; we serialize back to a
    /// string for the unified `ToolCall.arguments` slot.
    #[serde(default)]
    pub(crate) params: serde_json::Value,
    /// Free-form result text. Schema varies — we tolerate either a string
    /// or a structured object.
    #[serde(default)]
    pub(crate) result: serde_json::Value,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolCallData {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) arguments: serde_json::Value,
    #[serde(default)]
    pub(crate) result: serde_json::Value,
    pub(crate) status: Option<String>,
}

pub(crate) fn millis_to_datetime(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}
