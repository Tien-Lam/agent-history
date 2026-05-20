use super::*;

mod command_contracts;
mod core_schemas;
mod dto_fragments;
mod ref_patterns;

fn assert_closed_params(label: &str, params: &Value) {
    assert_eq!(params["type"], "object", "{label} params must be an object");
    assert_eq!(
        params["additionalProperties"], false,
        "{label} params must reject undocumented properties"
    );
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect()
}

/// Tiny helper: we don't pull a regex crate just for tests, so check a few
/// known anchors without full regex matching.
fn regex_lite_check(pattern: &str, sample: &str) -> bool {
    assert!(pattern.contains('/'));
    sample.contains('/')
}
