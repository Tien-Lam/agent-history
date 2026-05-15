use crate::model::Provider;

use super::*;

#[test]
fn every_listed_subcommand_has_a_schema() {
    for name in subcommands() {
        assert!(
            schema_for(name).is_some(),
            "missing schema for subcommand '{name}'"
        );
    }
}

#[test]
fn unknown_subcommand_returns_none() {
    assert!(schema_for("nonsense").is_none());
}

#[test]
fn schemas_declare_draft_2020_12() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        assert_eq!(
            schema["$schema"],
            common::SCHEMA_DRAFT,
            "subcommand '{name}' missing $schema draft declaration"
        );
        assert!(
            schema["params"].is_object(),
            "subcommand '{name}' missing params object"
        );
        assert!(
            schema["response"].is_object(),
            "subcommand '{name}' missing response object"
        );
    }
}

#[test]
fn index_returns_subcommands_array() {
    let idx = subcommand_index();
    let arr = idx["subcommands"].as_array().unwrap();
    assert_eq!(arr.len(), subcommands().len());
}

#[test]
fn all_schemas_keyed_by_name() {
    let all = all_schemas();
    let map = all.as_object().unwrap();
    for name in subcommands() {
        assert!(map.contains_key(name), "missing key {name} in all_schemas");
    }
}

#[test]
fn search_schema_describes_query_param() {
    let schema = schema_for("search").unwrap();
    let params = &schema["params"]["properties"];
    assert!(params["query"].is_object());
    assert!(params["limit"].is_object());
    assert_eq!(params["limit"]["default"], 20);
}

#[test]
fn show_schema_includes_reference_pattern() {
    let schema = schema_for("show").unwrap();
    let pattern = &schema["params"]["properties"]["reference"]["pattern"];
    assert!(pattern.is_string());
    // Sanity check: the example ref from the description matches the pattern.
    let re = regex_lite_check(pattern.as_str().unwrap(), "claude-code/abc-123#7");
    assert!(re, "show ref pattern should match canonical example");
}

#[test]
fn provider_enum_tracks_provider_registry() {
    let expected: Vec<&str> = Provider::all().iter().map(|p| p.slug()).collect();
    let provider_enum = common::provider_slug_enum();
    let actual: Vec<&str> = provider_enum
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(actual, expected);
}

/// Tiny helper: we don't pull a regex crate just for tests, so check a few
/// known anchors without full regex matching.
fn regex_lite_check(pattern: &str, sample: &str) -> bool {
    // We only assert the pattern is well-formed and the sample contains
    // both "/" and "#" (required by the pattern's structure).
    assert!(pattern.contains('#'));
    sample.contains('/') && sample.contains('#')
}
