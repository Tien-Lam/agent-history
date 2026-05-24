use super::*;

#[test]
fn search_schema_describes_query_param() {
    let schema = schema_for("search").unwrap();
    let params = &schema["params"]["properties"];
    assert!(params["query"].is_object());
    assert!(params["limit"].is_object());
    assert!(params["debug_search"].is_object());
    assert_eq!(
        params["limit"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_LIMIT_DEFAULT)
    );
    assert_eq!(
        params["limit"]["maximum"],
        serde_json::json!(crate::schema_fragments::SEARCH_LIMIT_MAX)
    );
    assert!(params["cursor"].is_object());
    assert_eq!(
        params["watch_interval_ms"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_WATCH_INTERVAL_MS_DEFAULT)
    );
    assert_eq!(
        params["watch_iterations"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_WATCH_ITERATIONS_DEFAULT)
    );
    assert_eq!(
        params["hybrid_weight"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_HYBRID_WEIGHT_DEFAULT)
    );
}

#[test]
fn list_schema_describes_pagination_params() {
    let schema = schema_for("list").unwrap();
    let params = &schema["params"]["properties"];
    assert!(params["limit"].is_object());
    assert_eq!(
        params["limit"]["default"],
        serde_json::json!(crate::schema_fragments::LIST_LIMIT_DEFAULT)
    );
    assert_eq!(
        params["limit"]["maximum"],
        serde_json::json!(crate::schema_fragments::LIST_LIMIT_MAX)
    );
    assert!(params["cursor"].is_object());

    let json_response = &schema["response"]["oneOf"][0];
    assert_eq!(
        string_array(&json_response["required"]),
        vec!["sessions", "meta"]
    );
    assert_eq!(
        string_array(&json_response["properties"]["meta"]["required"]),
        vec!["next_cursor", "total"]
    );
}

#[test]
fn search_schema_describes_json_envelope() {
    let schema = schema_for("search").unwrap();
    let response = &schema["response"];
    assert_eq!(response["type"], "object");
    assert_eq!(string_array(&response["required"]), vec!["hits", "meta"]);

    let variants = response["properties"]["hits"]["items"]["oneOf"]
        .as_array()
        .expect("search hit schema should distinguish message and note hits");
    assert_eq!(variants.len(), 2);
    let message_hit = &variants[0];
    let note_hit = &variants[1];
    assert_eq!(
        message_hit["properties"]["kind"]["enum"],
        serde_json::json!(["message"])
    );
    assert_eq!(
        note_hit["properties"]["kind"]["enum"],
        serde_json::json!(["note"])
    );
    for field in [
        "kind",
        "session_id",
        "message_id",
        "score",
        "snippet",
        "provider",
        "project",
        "started_at",
        "source",
    ] {
        assert!(
            string_array(&message_hit["required"]).contains(&field),
            "search message hit schema missing required field {field}"
        );
        assert!(
            string_array(&note_hit["required"]).contains(&field),
            "search note hit schema missing required field {field}"
        );
    }
    assert!(message_hit["properties"]["ref"]["pattern"].is_string());
    assert!(message_hit["properties"]["turn"].is_object());
    assert!(message_hit["properties"]["explanation"].is_object());
    assert!(message_hit["properties"].get("note_id").is_none());
    assert!(note_hit["properties"]["note_id"].is_object());
    assert!(note_hit["properties"]["ref"]["pattern"].is_string());
    assert!(note_hit["properties"].get("turn").is_none());

    let meta = &response["properties"]["meta"];
    assert_eq!(meta["type"], "object");
    assert_eq!(
        string_array(&meta["required"]),
        vec!["next_cursor", "total", "engine"]
    );
    assert_eq!(
        meta["properties"]["engine"]["enum"],
        serde_json::json!(["lexical", "hybrid"])
    );
}

#[test]
fn index_schema_describes_summary_contract() {
    let schema = schema_for("index").unwrap();
    let response = &schema["response"];
    assert_eq!(response["additionalProperties"], false);
    for field in [
        "status",
        "providers",
        "sessions_total",
        "added",
        "updated",
        "unchanged",
        "removed",
        "messages_indexed",
        "force",
        "index_dir",
        "duration_ms",
        "errors",
        "embeddings",
    ] {
        assert!(
            string_array(&response["required"]).contains(&field),
            "index schema missing required summary field {field}"
        );
        assert!(
            response["properties"][field].is_object(),
            "index schema missing property for {field}"
        );
    }
    let variants = response["properties"]["embeddings"]["oneOf"]
        .as_array()
        .expect("embeddings schema variants");
    let statuses: Vec<&str> = variants
        .iter()
        .map(|variant| {
            variant["properties"]["status"]["const"]
                .as_str()
                .expect("status const")
        })
        .collect();
    assert_eq!(statuses, vec!["disabled", "awaiting-consent", "enabled"]);
    assert!(
        variants[2]["properties"]["messages_total_in_store"].is_object(),
        "enabled embeddings schema should document cache counters"
    );
}

#[test]
fn sources_schema_describes_remote_registry_subcommands() {
    let schema = schema_for("sources").unwrap();
    let subcommands = schema["subcommands"]
        .as_object()
        .expect("sources schema subcommands");

    for name in ["add", "list", "remove", "pull"] {
        assert!(
            subcommands.contains_key(name),
            "sources schema missing subcommand {name}"
        );
        assert!(
            subcommands[name]["params"].is_object(),
            "sources {name} missing params schema"
        );
        assert!(
            subcommands[name]["response"].is_object(),
            "sources {name} missing response schema"
        );
    }

    assert_eq!(
        string_array(&subcommands["add"]["params"]["required"]),
        vec!["name", "host", "path"]
    );
    assert_eq!(
        string_array(&subcommands["remove"]["params"]["required"]),
        vec!["name"]
    );
    assert_eq!(
        subcommands["add"]["params"]["properties"]["name"]["pattern"],
        "^(?!local$)[A-Za-z0-9][A-Za-z0-9_-]*$"
    );
    assert_eq!(
        subcommands["remove"]["params"]["properties"]["name"]["pattern"],
        "^(?!local$)[A-Za-z0-9][A-Za-z0-9_-]*$"
    );
    assert!(subcommands["pull"]["params"]["properties"]["dry_run"].is_object());
    assert!(subcommands["pull"]["response"]["oneOf"].is_array());
}

#[test]
fn health_schema_documents_provider_fidelity_contract() {
    let schema = schema_for("health").unwrap();
    let response = &schema["response"];
    assert_eq!(response["additionalProperties"], false);
    assert!(
        string_array(&response["required"]).contains(&"provider_fidelity"),
        "health response must require provider_fidelity"
    );

    let row = &response["properties"]["provider_fidelity"]["items"];
    assert_eq!(row["additionalProperties"], false);
    for field in [
        "label",
        "provider",
        "session_count",
        "message_count",
        "parse",
        "blocks",
        "tool_call_fidelity",
    ] {
        assert!(
            string_array(&row["required"]).contains(&field),
            "provider fidelity row missing required field {field}"
        );
    }

    assert_eq!(
        string_array(&row["properties"]["parse"]["required"]),
        vec![
            "records_seen",
            "parse_errors",
            "skipped_records",
            "empty_content"
        ]
    );
    assert!(row["properties"]["tool_call_fidelity"]["properties"]["invalid_json_args"].is_object());
}

#[test]
fn show_schema_includes_reference_pattern() {
    let schema = schema_for("show").unwrap();
    let pattern = &schema["params"]["properties"]["reference"]["pattern"];
    assert!(pattern.is_string());
    let re = regex_lite_check(pattern.as_str().unwrap(), "claude-code/abc-123#7");
    assert!(re, "show ref pattern should match canonical example");
    let re = regex_lite_check(pattern.as_str().unwrap(), "laptop:claude-code/abc-123#7");
    assert!(re, "show ref pattern should match source-qualified refs");
    assert!(
        pattern.as_str().unwrap().contains(":)?"),
        "show ref pattern should document the optional source prefix"
    );
    assert!(
        !pattern.as_str().unwrap().ends_with(")?$"),
        "show ref pattern should require a turn suffix"
    );
}

#[test]
fn diff_schema_includes_source_qualified_session_pattern() {
    let schema = schema_for("diff").unwrap();
    let pattern = schema["params"]["properties"]["session1"]["pattern"]
        .as_str()
        .unwrap();
    assert!(regex_lite_check(pattern, "claude-code/abc-123"));
    assert!(regex_lite_check(pattern, "laptop:claude-code/abc-123"));
    assert!(
        pattern.ends_with("/[^#]+$"),
        "diff session ref pattern should reject turn suffixes"
    );
}

#[test]
fn citation_ref_outputs_require_turn_suffixes() {
    let citation_pattern = common::source_qualified_citation_ref_pattern();

    for (schema_name, path) in [
        (
            "decisions",
            "response.oneOf.0.properties.decisions.items.properties.ref.pattern",
        ),
        (
            "decisions",
            "response.oneOf.1.properties.decisions.items.properties.ref.pattern",
        ),
        (
            "todos",
            "response.oneOf.0.properties.todos.items.properties.ref.pattern",
        ),
        (
            "todos",
            "response.oneOf.1.properties.todos.items.properties.ref.pattern",
        ),
        (
            "project",
            "response.properties.decisions.items.properties.ref.pattern",
        ),
        (
            "project",
            "response.properties.todos.items.properties.ref.pattern",
        ),
        (
            "report",
            "response.properties.decisions.items.properties.ref.pattern",
        ),
        (
            "report",
            "response.properties.todos.items.properties.ref.pattern",
        ),
    ] {
        let schema = schema_for(schema_name).unwrap();
        let pattern = schema_path(&schema, path).as_str().unwrap();
        assert_eq!(
            pattern, citation_pattern,
            "{schema_name} {path} should require a citation ref with #turn"
        );
    }

    let todos = schema_for("todos").unwrap();
    let target_session_pattern = schema_path(
        &todos,
        "response.oneOf.1.properties.todos.items.properties.target_session.pattern",
    )
    .as_str()
    .unwrap();
    assert_eq!(
        target_session_pattern,
        common::todo_target_ref_pattern(),
        "target_session may be a session-level provider ref or beads-style id"
    );
}

#[test]
fn session_ref_outputs_reject_turn_suffixes() {
    let session_only_pattern = common::source_qualified_session_only_ref_pattern();

    for (schema_name, path) in [
        (
            "threads",
            "response.oneOf.0.properties.threads.items.properties.session_refs.items.pattern",
        ),
        (
            "threads",
            "response.oneOf.1.properties.threads.items.properties.member_refs.items.pattern",
        ),
        (
            "track",
            "response.properties.timeline.items.properties.session_ref.pattern",
        ),
        (
            "project",
            "response.properties.threads.items.properties.session_refs.items.pattern",
        ),
        (
            "report",
            "response.properties.threads.items.properties.session_refs.items.pattern",
        ),
    ] {
        let schema = schema_for(schema_name).unwrap();
        let pattern = schema_path(&schema, path).as_str().unwrap();
        assert_eq!(
            pattern, session_only_pattern,
            "{schema_name} {path} should require a session ref without #turn"
        );
    }
}

fn schema_path<'a>(schema: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    let mut current = schema;
    for segment in path.split('.') {
        current = if let Ok(index) = segment.parse::<usize>() {
            &current[index]
        } else {
            &current[segment]
        };
    }
    current
}
