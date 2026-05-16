use serde_json::Value;

pub(crate) fn string_or_object_field(value: &Value, fields: &[&str]) -> String {
    direct_string_or_object_field(value, fields).unwrap_or_default()
}

pub(crate) fn string_or_object_field_or_pretty(value: &Value, fields: &[&str]) -> String {
    direct_string_or_object_field(value, fields).unwrap_or_else(|| pretty_json(value))
}

pub(crate) fn string_or_typed_text_array(
    value: &Value,
    type_name: &str,
    text_field: &str,
) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some(type_name) {
                    item.get(text_field)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn direct_string_or_object_field(value: &Value, fields: &[&str]) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    let Value::Object(map) = value else {
        return None;
    };

    fields
        .iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str).map(str::to_owned))
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
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
}
