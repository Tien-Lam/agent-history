use super::*;

#[test]
fn tools_list_advertises_all_tools() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "search_sessions",
        "list_sessions",
        "get_session",
        "get_message",
        "reindex",
        "health",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected} in {names:?}"
        );
    }
}

#[test]
fn tool_definitions_are_stable_and_closed() {
    let tools = tool_definitions();
    let tools = tools.as_array().unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "search_sessions",
            "list_sessions",
            "get_session",
            "get_message",
            "reindex",
            "health",
        ]
    );

    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let input_schema = &tool["inputSchema"];
        assert_eq!(input_schema["type"], "object", "{name} inputSchema type");
        assert!(
            input_schema["properties"].is_object(),
            "{name} inputSchema properties"
        );
        assert_eq!(
            input_schema["additionalProperties"], false,
            "{name} inputSchema should reject undocumented arguments"
        );
    }
}

#[test]
fn tool_provider_enums_track_provider_registry() {
    let expected: Vec<String> = Provider::all()
        .iter()
        .map(|provider| provider.slug().to_string())
        .collect();
    let tools = tool_definitions();
    let tools = tools.as_array().unwrap();

    for tool_name in ["list_sessions", "get_session", "reindex"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"].as_str() == Some(tool_name))
            .unwrap_or_else(|| panic!("missing tool {tool_name}"));
        let actual: Vec<String> = tool["inputSchema"]["properties"]["provider"]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("missing provider enum for {tool_name}"))
            .iter()
            .map(|slug| slug.as_str().unwrap().to_string())
            .collect();
        assert_eq!(actual, expected, "{tool_name} provider enum drifted");
    }
}

#[test]
fn tool_ref_patterns_use_shared_contract_fragments() {
    let tools = tool_definitions();
    let tools = tools.as_array().unwrap();
    let get_message = tools
        .iter()
        .find(|tool| tool["name"].as_str() == Some("get_message"))
        .expect("missing get_message tool");

    assert_eq!(
        get_message["inputSchema"]["properties"]["ref"]["pattern"],
        schema_fragments::source_qualified_citation_ref_pattern()
    );
}

#[test]
fn tool_limit_schemas_use_shared_contract_constants() {
    let tools = tool_definitions();
    let tools = tools.as_array().unwrap();

    let search = tool_by_name(tools, "search_sessions");
    let search_limit = &search["inputSchema"]["properties"]["limit"];
    assert_eq!(
        search_limit["default"],
        serde_json::json!(schema_fragments::SEARCH_LIMIT_DEFAULT)
    );
    assert_eq!(
        search_limit["maximum"],
        serde_json::json!(schema_fragments::MCP_SEARCH_LIMIT_MAX)
    );

    let list = tool_by_name(tools, "list_sessions");
    let list_limit = &list["inputSchema"]["properties"]["limit"];
    assert_eq!(
        list_limit["default"],
        serde_json::json!(schema_fragments::MCP_LIST_LIMIT_DEFAULT)
    );
    assert_eq!(
        list_limit["maximum"],
        serde_json::json!(schema_fragments::MCP_LIST_LIMIT_MAX)
    );

    let get_message = tool_by_name(tools, "get_message");
    let include_context = &get_message["inputSchema"]["properties"]["include_context"];
    assert_eq!(
        include_context["default"],
        serde_json::json!(schema_fragments::MCP_INCLUDE_CONTEXT_DEFAULT)
    );
    assert_eq!(
        include_context["maximum"],
        serde_json::json!(schema_fragments::MCP_INCLUDE_CONTEXT_MAX)
    );
}

#[test]
fn tool_output_schemas_use_shared_registry() {
    let tools = tool_definitions();
    let tools = tools.as_array().unwrap();

    let mut names_with_output_schema = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        if let Some(output_schema) = tool.get("outputSchema") {
            names_with_output_schema.push(name);
            assert_eq!(
                output_schema,
                &schema_fragments::mcp_tool_output_schema(name)
                    .unwrap_or_else(|| panic!("missing registered output schema for {name}")),
                "{name} output schema should come from the shared registry"
            );
        }
    }

    assert_eq!(
        names_with_output_schema,
        vec![
            "search_sessions",
            "list_sessions",
            "get_session",
            "get_message",
            "reindex",
            "health",
        ]
    );
}
