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
fn stringish_extracts_nested_object_fields_and_scalars() {
    assert_eq!(
        stringish(
            Some(&json!({"model": {"id": "gpt-object"}})),
            &["model", "id"]
        ),
        Some("gpt-object".to_string())
    );
    assert_eq!(stringish(Some(&json!(42)), &["id"]), Some("42".to_string()));
}

#[test]
fn value_helpers_accept_strings_and_nested_objects() {
    assert_eq!(value_i64(Some(&json!({"value": "42"}))), Some(42));
    assert_eq!(value_u64(Some(&json!({"tokens": "7"}))), Some(7));
    assert_eq!(
        value_bool(Some(&json!({"success": "true"})), &["success", "value"]),
        Some(true)
    );
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
