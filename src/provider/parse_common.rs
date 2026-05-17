use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::model::{ContentBlock, TokenUsage, ToolCall, ToolResult};
use crate::provider::json_text::pretty_json;

pub(crate) fn parse_utc(raw: &str) -> Option<DateTime<Utc>> {
    raw.parse::<DateTime<Utc>>().ok()
}

pub(crate) fn parse_utc_opt(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(parse_utc)
}

pub(crate) fn parse_utc_or_now(raw: Option<&str>) -> DateTime<Utc> {
    parse_utc_opt(raw).unwrap_or_else(Utc::now)
}

pub(crate) fn millis_to_utc(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}

pub(crate) fn token_usage(
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
) -> TokenUsage {
    TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    }
}

pub(crate) fn nonzero_token_usage(
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
) -> Option<TokenUsage> {
    if input_tokens > 0 || output_tokens > 0 {
        Some(token_usage(
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
        ))
    } else {
        None
    }
}

pub(crate) fn pretty_json_opt(value: Option<&Value>) -> String {
    value.map(pretty_json).unwrap_or_default()
}

pub(crate) fn tool_use_block(
    id: impl Into<String>,
    name: impl Into<String>,
    arguments: impl Into<String>,
) -> ContentBlock {
    ContentBlock::ToolUse(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: arguments.into(),
    })
}

pub(crate) fn tool_result_block(
    tool_call_id: impl Into<String>,
    success: bool,
    output: impl Into<String>,
) -> ContentBlock {
    ContentBlock::ToolResult(ToolResult {
        tool_call_id: tool_call_id.into(),
        success,
        output: output.into(),
    })
}
