use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::provider::parse_common::{
    deserialize_optional_struct_skip_invalid, deserialize_vec_skip_invalid, timestamp_value_to_utc,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ComposerData {
    #[serde(rename = "composerId")]
    pub(crate) composer_id: Option<Value>,
    pub(crate) name: Option<Value>,
    #[serde(rename = "lastUpdatedAt")]
    pub(crate) last_updated_at: Option<Value>,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<Value>,
    /// Newer Cursor builds embed bubble headers in the composer record.
    /// Each entry has `bubbleId` and a `type` (1 = user, 2 = assistant).
    #[serde(
        rename = "fullConversationHeadersOnly",
        default,
        deserialize_with = "deserialize_vec_skip_invalid"
    )]
    pub(crate) headers: Vec<HeaderEntry>,
    /// Some builds expose the working directory directly.
    #[serde(rename = "currentWorkspaceFolder")]
    pub(crate) workspace_folder: Option<Value>,
    /// Newer schema may carry the model name on the composer record.
    pub(crate) model: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HeaderEntry {
    #[serde(rename = "bubbleId")]
    pub(crate) bubble_id: Option<Value>,
    #[serde(rename = "type")]
    pub(crate) bubble_type: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BubbleData {
    #[serde(rename = "type")]
    pub(crate) bubble_type: Option<Value>,
    pub(crate) text: Option<Value>,
    /// Older Cursor builds put the message body under `richText` markdown.
    #[serde(rename = "richText")]
    pub(crate) rich_text: Option<Value>,
    /// Inline code blocks attached to the bubble.
    #[serde(
        rename = "codeBlocks",
        default,
        deserialize_with = "deserialize_vec_skip_invalid"
    )]
    pub(crate) code_blocks: Vec<CodeBlockData>,
    /// Legacy single tool-call structure.
    #[serde(
        rename = "toolFormerData",
        default,
        deserialize_with = "deserialize_optional_struct_skip_invalid"
    )]
    pub(crate) tool_former: Option<ToolFormerData>,
    /// Newer multi-tool-call structure.
    #[serde(
        default,
        rename = "toolCalls",
        deserialize_with = "deserialize_vec_skip_invalid"
    )]
    pub(crate) tool_calls: Vec<ToolCallData>,
    /// Per-bubble timestamp (newer builds).
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<Value>,
    /// Per-bubble model attribution (newer builds).
    pub(crate) model: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodeBlockData {
    #[serde(rename = "languageId")]
    pub(crate) language: Option<Value>,
    pub(crate) code: Option<Value>,
    /// Some builds use `content` for the code body.
    pub(crate) content: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolFormerData {
    #[serde(rename = "toolCallId")]
    pub(crate) tool_call_id: Option<Value>,
    pub(crate) name: Option<Value>,
    /// Cursor stores arguments as a JSON object; we serialize back to a
    /// string for the unified `ToolCall.arguments` slot.
    #[serde(default)]
    pub(crate) params: serde_json::Value,
    /// Free-form result text. Schema varies — we tolerate either a string
    /// or a structured object.
    #[serde(default)]
    pub(crate) result: serde_json::Value,
    pub(crate) status: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolCallData {
    pub(crate) id: Option<Value>,
    pub(crate) name: Option<Value>,
    #[serde(default)]
    pub(crate) arguments: serde_json::Value,
    #[serde(default)]
    pub(crate) result: serde_json::Value,
    pub(crate) status: Option<Value>,
}

pub(crate) fn millis_value_to_datetime(value: &Value) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(
        Some(value),
        &["createdAt", "lastUpdatedAt", "timestamp", "value"],
    )
}
