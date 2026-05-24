use serde_json::{json, Map, Value};

use crate::model::Provider;

pub(crate) type SchemaProperties = Map<String, Value>;

fn provider_slug_pattern() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn source_qualified_session_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+(#[1-9][0-9]*)?$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_session_only_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+$",
        provider_slug_pattern()
    )
}

pub(crate) fn todo_target_ref_pattern() -> String {
    format!(
        "^(([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+|[a-z]{{2,}}-[a-z0-9.]*[0-9][a-z0-9.]*)$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_citation_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+#[1-9][0-9]*$",
        provider_slug_pattern()
    )
}

pub(crate) fn mcp_session_uri_pattern() -> String {
    format!(
        "^aghist://(session|source/[A-Za-z0-9][A-Za-z0-9_-]*/session)/({})/.+$",
        provider_slug_pattern()
    )
}

pub(crate) fn mcp_turn_uri_pattern() -> String {
    format!(
        "^aghist://(session|source/[A-Za-z0-9][A-Za-z0-9_-]*/session)/({})/.+/turn/[1-9][0-9]*$",
        provider_slug_pattern()
    )
}

pub(crate) fn provider_slug_enum() -> Value {
    json!(Provider::all().iter().map(|p| p.slug()).collect::<Vec<_>>())
}

pub(crate) fn provider_slug_enum_nullable() -> Value {
    let mut slugs: Vec<Value> = Provider::all().iter().map(|p| json!(p.slug())).collect();
    slugs.push(Value::Null);
    Value::Array(slugs)
}

pub(crate) fn schema_props(
    entries: impl IntoIterator<Item = (&'static str, Value)>,
) -> SchemaProperties {
    entries
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect()
}

pub(crate) fn object_schema(properties: SchemaProperties, required: &[&str]) -> Value {
    object_schema_with(properties, required, false)
}

pub(crate) fn closed_object_schema(properties: SchemaProperties, required: &[&str]) -> Value {
    object_schema_with(properties, required, true)
}

fn object_schema_with(properties: SchemaProperties, required: &[&str], closed: bool) -> Value {
    let mut schema = schema_props([
        ("type", json!("object")),
        ("properties", Value::Object(properties)),
    ]);
    if !required.is_empty() {
        schema.insert("required".to_string(), json!(required));
    }
    if closed {
        schema.insert("additionalProperties".to_string(), json!(false));
    }
    Value::Object(schema)
}

pub(crate) fn closed_empty_object_schema() -> Value {
    closed_object_schema(SchemaProperties::new(), &[])
}

pub(crate) fn array_schema(items: Value) -> Value {
    let mut schema = schema_props([("type", json!("array"))]);
    schema.insert("items".to_string(), items);
    Value::Object(schema)
}

pub(crate) fn with_description(mut schema: Value, description: &str) -> Value {
    if let Value::Object(properties) = &mut schema {
        properties.insert("description".to_string(), json!(description));
    }
    schema
}
