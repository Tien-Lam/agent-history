use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::model::{ContentBlock, TokenUsage, ToolCall, ToolResult};
use crate::provider::json_text::pretty_json;
use crate::provider::ProviderError;

pub(crate) struct JsonlRecord<T> {
    pub line_number: usize,
    pub value: T,
}

pub(crate) struct JsonlError {
    pub line_number: usize,
    pub error: serde_json::Error,
}

pub(crate) struct JsonlScan<T> {
    pub records: Vec<JsonlRecord<T>>,
}

pub(crate) struct JsonlStats {
    pub line_count: usize,
    pub parse_errors: usize,
}

pub(crate) fn visit_jsonl_records<T, F, E>(
    path: &Path,
    mut on_record: F,
    mut on_error: E,
) -> Result<JsonlStats, ProviderError>
where
    T: DeserializeOwned,
    F: FnMut(JsonlRecord<T>),
    E: FnMut(JsonlError),
{
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut line_count = 0;
    let mut parse_errors = 0;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let line_number = idx + 1;
        line_count += 1;
        match serde_json::from_str(&line) {
            Ok(value) => on_record(JsonlRecord { line_number, value }),
            Err(error) => {
                parse_errors += 1;
                on_error(JsonlError { line_number, error });
            }
        }
    }

    Ok(JsonlStats {
        line_count,
        parse_errors,
    })
}

pub(crate) fn parse_jsonl_records<T>(path: &Path) -> Result<JsonlScan<T>, ProviderError>
where
    T: DeserializeOwned,
{
    let mut records = Vec::new();
    visit_jsonl_records(path, |record| records.push(record), |_| {})?;

    Ok(JsonlScan { records })
}

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

pub(crate) fn file_modified_utc(path: &Path) -> Option<DateTime<Utc>> {
    path.metadata()
        .and_then(|m| m.modified())
        .map(DateTime::<Utc>::from)
        .ok()
}

pub(crate) fn timestamp_with_index_millis(base: DateTime<Utc>, idx: usize) -> DateTime<Utc> {
    let offset = i64::try_from(idx).unwrap_or(i64::MAX);
    base.checked_add_signed(Duration::milliseconds(offset))
        .unwrap_or(base)
}

pub(crate) fn epoch_timestamp_for_index(idx: usize) -> DateTime<Utc> {
    Utc.timestamp_opt(i64::try_from(idx).unwrap_or(i64::MAX), 0)
        .single()
        .unwrap_or_else(Utc::now)
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

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Row {
        value: String,
    }

    #[test]
    fn visit_jsonl_records_skips_blank_lines_and_tracks_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.jsonl");
        std::fs::write(
            &path,
            "{\"value\":\"one\"}\n\nnot-json\n{\"value\":\"two\"}\n",
        )
        .unwrap();

        let mut records = Vec::new();
        let mut errors = Vec::new();
        let stats = visit_jsonl_records::<Row, _, _>(
            &path,
            |record| records.push(record),
            |error| errors.push(error),
        )
        .unwrap();

        assert_eq!(stats.line_count, 3);
        assert_eq!(stats.parse_errors, 1);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].line_number, 1);
        assert_eq!(records[0].value.value, "one");
        assert_eq!(records[1].line_number, 4);
        assert_eq!(records[1].value.value, "two");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line_number, 3);
    }

    #[test]
    fn timestamp_with_index_millis_preserves_order_without_panicking_on_huge_idx() {
        let base = parse_utc("2026-01-01T00:00:00Z").unwrap();

        assert_eq!(
            timestamp_with_index_millis(base, 7),
            parse_utc("2026-01-01T00:00:00.007Z").unwrap()
        );
        assert_eq!(timestamp_with_index_millis(base, usize::MAX), base);
    }
}
