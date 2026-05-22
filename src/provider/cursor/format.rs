use chrono::{DateTime, TimeZone, Utc};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::provider::json_text::string_or_object_field_or_pretty;

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
        deserialize_with = "deserialize_optional_struct"
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

pub(crate) fn millis_to_datetime(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}

pub(crate) fn millis_value_to_datetime(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|n| i64::try_from(n).ok()))
            .and_then(millis_to_datetime),
        Value::String(text) => text.parse::<i64>().ok().and_then(millis_to_datetime),
        Value::Object(map) => ["createdAt", "lastUpdatedAt", "timestamp", "value"]
            .iter()
            .find_map(|field| map.get(*field).and_then(millis_value_to_datetime)),
        _ => None,
    }
}

pub(crate) fn value_u8(value: &Value) -> Option<u8> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .or_else(|| number.as_i64().and_then(|n| u8::try_from(n).ok())),
        Value::String(text) => text.parse::<u8>().ok(),
        Value::Object(map) => ["type", "value"]
            .iter()
            .find_map(|field| map.get(*field).and_then(value_u8)),
        _ => None,
    }
}

pub(crate) fn optional_string(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(map) => object_fields
            .iter()
            .find_map(|field| optional_string(map.get(*field), object_fields)),
        _ => None,
    }
}

pub(crate) fn optional_text(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let text = string_or_object_field_or_pretty(value?, object_fields);
    (!text.is_empty()).then_some(text)
}

fn deserialize_vec_skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };

    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };

    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

fn deserialize_optional_struct<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    Ok(serde_json::from_value(value).ok())
}
