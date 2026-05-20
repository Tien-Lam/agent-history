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
fn schema_command_fields_match_registry_names() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        let expected = if name == "list" { "--list" } else { name };
        assert_eq!(
            schema["command"].as_str(),
            Some(expected),
            "subcommand '{name}' has a mismatched command field"
        );
    }
}

#[test]
fn schema_params_are_closed_objects() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        assert_closed_params(name, &schema["params"]);

        if let Some(subcommands) = schema["subcommands"].as_object() {
            for (subcommand, schema) in subcommands {
                assert_closed_params(&format!("{name} {subcommand}"), &schema["params"]);
            }
        }
    }
}
