use serde_json::Value;

pub(crate) fn string_or_object_field(value: &Value, fields: &[&str]) -> String {
    direct_string_or_object_field(value, fields).unwrap_or_default()
}

pub(crate) fn string_or_object_field_or_pretty(value: &Value, fields: &[&str]) -> String {
    direct_string_or_object_field(value, fields).unwrap_or_else(|| pretty_json(value))
}

pub(crate) fn non_empty_string_or_object_field_or_pretty(
    value: Option<&Value>,
    fields: &[&str],
) -> Option<String> {
    let text = string_or_object_field_or_pretty(value?, fields);
    (!text.is_empty()).then_some(text)
}

pub(crate) fn string_or_pretty(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| pretty_json(value), str::to_owned)
}

pub(crate) fn stringish(value: Option<&Value>, object_fields: &[&str]) -> Option<String> {
    let value = value?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(map) => object_fields
            .iter()
            .find_map(|field| stringish(map.get(*field), object_fields))
            .or_else(|| {
                let text = string_or_object_field(value, object_fields);
                (!text.is_empty()).then_some(text)
            }),
        _ => None,
    }
}

pub(crate) fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|n| i64::try_from(n).ok())),
        Value::String(text) => text.parse::<i64>().ok(),
        Value::Object(map) => ["value", "timestamp", "createdAt", "created", "updated"]
            .iter()
            .find_map(|field| value_i64(map.get(*field))),
        _ => None,
    }
}

pub(crate) fn value_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number
            .as_u64()
            .or_else(|| number.as_i64().and_then(|n| u64::try_from(n).ok())),
        Value::String(text) => text.parse::<u64>().ok(),
        Value::Object(map) => ["value", "tokens", "count"]
            .iter()
            .find_map(|field| value_u64(map.get(*field))),
        _ => None,
    }
}

pub(crate) fn value_u8(value: Option<&Value>, object_fields: &[&str]) -> Option<u8> {
    match value? {
        Value::Number(number) => number
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .or_else(|| number.as_i64().and_then(|n| u8::try_from(n).ok())),
        Value::String(text) => text.parse::<u8>().ok(),
        Value::Object(map) => object_fields
            .iter()
            .find_map(|field| value_u8(map.get(*field), object_fields)),
        _ => None,
    }
}

pub(crate) fn value_bool(value: Option<&Value>, object_fields: &[&str]) -> Option<bool> {
    let value = value?;
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => text.parse::<bool>().ok(),
        Value::Object(map) => object_fields
            .iter()
            .find_map(|field| value_bool(map.get(*field), object_fields)),
        _ => None,
    }
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

pub(crate) fn string_or_typed_text_array_or_pretty(
    value: &Value,
    type_name: &str,
    text_field: &str,
) -> String {
    match value {
        Value::String(_) | Value::Array(_) => {
            string_or_typed_text_array(value, type_name, text_field)
        }
        _ => pretty_json(value),
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

pub(crate) fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests;
