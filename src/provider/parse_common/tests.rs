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

#[test]
fn timestamp_value_to_utc_accepts_rfc3339_millis_and_nested_fields() {
    let timestamp = timestamp_value_to_utc(
        Some(&serde_json::json!("2026-01-01T00:00:00Z")),
        &["timestamp"],
    )
    .unwrap();
    assert_eq!(timestamp, parse_utc("2026-01-01T00:00:00Z").unwrap());

    let timestamp =
        timestamp_value_to_utc(Some(&serde_json::json!(1767225600123_i64)), &["timestamp"])
            .unwrap();
    assert_eq!(timestamp, parse_utc("2026-01-01T00:00:00.123Z").unwrap());

    let timestamp = timestamp_value_to_utc(
        Some(&serde_json::json!({"createdAt": "1767225600007"})),
        &["createdAt", "timestamp"],
    )
    .unwrap();
    assert_eq!(timestamp, parse_utc("2026-01-01T00:00:00.007Z").unwrap());
}

#[test]
fn token_usage_from_options_defaults_missing_counts() {
    let usage = token_usage_from_options(None, Some(7), Some(3), None);

    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.cache_read_tokens, Some(3));
    assert_eq!(usage.cache_write_tokens, None);
}
