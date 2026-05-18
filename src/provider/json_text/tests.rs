use serde_json::json;

use super::*;

#[test]
fn string_or_object_field_prefers_direct_string() {
    assert_eq!(
        string_or_object_field(&json!("direct output"), &["content"]),
        "direct output"
    );
}

#[test]
fn string_or_object_field_respects_field_order() {
    let value = json!({
        "content": "short",
        "detailedContent": "long",
    });
    assert_eq!(
        string_or_object_field(&value, &["detailedContent", "content"]),
        "long"
    );
}

#[test]
fn string_or_object_field_or_pretty_preserves_freeform_json() {
    let value = json!({ "nested": { "answer": 42 } });
    let extracted = string_or_object_field_or_pretty(&value, &["output"]);
    assert!(extracted.contains("\"nested\""));
    assert!(extracted.contains("\"answer\": 42"));
}

#[test]
fn string_or_pretty_prefers_direct_string() {
    assert_eq!(string_or_pretty(&json!("direct output")), "direct output");
}

#[test]
fn string_or_pretty_preserves_freeform_json() {
    let value = json!({ "nested": { "answer": 42 } });
    let extracted = string_or_pretty(&value);
    assert!(extracted.contains("\"nested\""));
    assert!(extracted.contains("\"answer\": 42"));
}

#[test]
fn string_or_typed_text_array_joins_matching_parts() {
    let value = json!([
        { "type": "text", "text": "first" },
        { "type": "image", "text": "skip" },
        { "type": "text", "text": "second" }
    ]);
    assert_eq!(
        string_or_typed_text_array(&value, "text", "text"),
        "first\nsecond"
    );
}

#[test]
fn string_or_typed_text_array_or_pretty_preserves_object_json() {
    let value = json!({ "nested": { "answer": 42 } });
    let extracted = string_or_typed_text_array_or_pretty(&value, "text", "text");
    assert!(extracted.contains("\"nested\""));
    assert!(extracted.contains("\"answer\": 42"));
}
