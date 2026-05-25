mod common;

use assert_cmd::Command;
use serde_json::Value;

fn aghist() -> Command {
    common::helpers::isolated_aghist("json-contract")
}

fn parse_stdout_json(output: &assert_cmd::assert::Assert) -> Value {
    common::cli::assert_stdout_json(output)
}

fn assert_json_snapshot(name: &'static str, value: &Value) {
    let pretty = serde_json::to_string_pretty(value).expect("value serializes as pretty JSON");
    insta::assert_snapshot!(name, pretty);
}

fn normalize_index_summary(summary: &mut Value) {
    summary["duration_ms"] = serde_json::json!(0);
    summary["index_dir"] = serde_json::json!("[index-dir]");
    if summary.get("embeddings").is_some() {
        summary["embeddings"] = serde_json::json!({
            "status": "[feature-dependent]"
        });
    }
}

fn normalize_search_scores(doc: &mut Value) {
    if let Some(hits) = doc.get_mut("hits").and_then(Value::as_array_mut) {
        for hit in hits {
            hit["score"] = serde_json::json!(0.0);
        }
    }
}

fn normalize_health_doc(doc: &mut Value) {
    if let Some(checks) = doc.get_mut("checks").and_then(Value::as_array_mut) {
        for check in checks {
            if check["name"] == "index-dir-writable" {
                check["message"] = serde_json::json!("index dir writable: [index-dir]");
            } else if check["name"] == "metadata-db-readable" {
                check["message"] = serde_json::json!(
                    "metadata db absent; will be created on first write: [metadata-db]"
                );
            }
        }
    }
}

#[cfg(unix)]
fn normalize_sources_pull_doc(doc: &mut Value) {
    doc["cache_dir"] = serde_json::json!("[sources-cache]");
    if let Some(results) = doc.get_mut("results").and_then(Value::as_array_mut) {
        for result in results {
            result["data_dir"] = serde_json::json!("[source-data-dir]");
            result["pulled_at"] = serde_json::json!("[timestamp]");
        }
    }
}

fn compact_mcp_tools_list_contract(result: &Value) -> Value {
    let mut compact = result.clone();
    let tools = compact["tools"]
        .as_array_mut()
        .expect("tools/list result has tools array");
    for tool in tools {
        let name = tool["name"]
            .as_str()
            .expect("tool definition has string name")
            .to_string();
        if tool.get("outputSchema").is_some() {
            tool["outputSchema"] = serde_json::json!({
                "$ref": format!("aghist:schema/mcp/tool-output/{name}")
            });
        }
    }
    compact
}

fn mcp_tools_output_schema_contract(result: &Value) -> Value {
    let tools = result["tools"]
        .as_array()
        .expect("tools/list result has tools array");
    let schemas = tools
        .iter()
        .map(|tool| {
            let name = tool["name"].as_str().expect("tool has string name");
            let output_schema = tool
                .get("outputSchema")
                .unwrap_or_else(|| panic!("{name} missing outputSchema"));
            (name.to_string(), output_schema.clone())
        })
        .collect();
    Value::Object(schemas)
}

fn mcp_tool_output_schema<'a>(tools_result: &'a Value, name: &str) -> &'a Value {
    tools_result["tools"]
        .as_array()
        .expect("tools/list result has tools array")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("tools/list missing {name}"))
        .get("outputSchema")
        .unwrap_or_else(|| panic!("tools/list missing outputSchema for {name}"))
}

fn command_schema(name: &str) -> Value {
    let output = aghist().args(["schema", name]).assert().success();
    parse_stdout_json(&output)
}

fn command_response_schema(name: &str) -> Value {
    let schema = command_schema(name);
    serde_json::json!({
        "$schema": schema["$schema"].clone(),
        "$id": format!("aghist:schema/{name}/response-test"),
        "$ref": "#/response",
        "response": schema["response"].clone(),
        "definitions": schema.get("definitions").cloned().unwrap_or_else(|| serde_json::json!({})),
    })
}

#[cfg(unix)]
fn command_subcommand_response_schema(name: &str, subcommand: &str) -> Value {
    let schema = command_schema(name);
    serde_json::json!({
        "$schema": schema["$schema"].clone(),
        "$id": format!("aghist:schema/{name}/{subcommand}/response-test"),
        "$ref": format!("#/subcommands/{subcommand}/response"),
        "subcommands": schema["subcommands"].clone(),
        "definitions": schema.get("definitions").cloned().unwrap_or_else(|| serde_json::json!({})),
    })
}

fn assert_json_schema_matches(schema: &Value, doc: &Value, label: &str) {
    let validator = jsonschema::validator_for(schema)
        .unwrap_or_else(|err| panic!("{label} schema failed to compile: {err}\n{schema:#}"));
    let errors = validator
        .iter_errors(doc)
        .map(|err| format!("{}: {err}", err.instance_path()))
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "{label} failed JSON Schema validation:\n{}\ninstance:\n{doc:#}\nschema:\n{schema:#}",
        errors.join("\n")
    );
}

fn assert_index_schema_matches_output(doc: &Value) {
    assert_json_schema_matches(&command_response_schema("index"), doc, "index response");
}

fn assert_search_schema_matches_output(doc: &Value) {
    assert_json_schema_matches(&command_response_schema("search"), doc, "search response");
    let hits = doc["hits"].as_array().expect("search hits array");
    assert!(!hits.is_empty(), "contract fixture should produce a hit");
}

fn assert_health_fixture_is_exercised(doc: &Value) {
    let checks = doc["checks"].as_array().expect("health checks array");
    assert!(!checks.is_empty(), "contract fixture should produce checks");

    let fidelity = doc["provider_fidelity"]
        .as_array()
        .expect("health provider_fidelity array");
    assert!(
        !fidelity.is_empty(),
        "contract fixture should produce provider fidelity"
    );
}

#[cfg(unix)]
fn write_contract_rsync(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = dir.join("fake-rsync.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         dest=\n\
         for a in \"$@\"; do dest=\"$a\"; done\n\
         mkdir -p \"$dest\"\n\
         printf 'contract-jsonl' > \"$dest/sample.jsonl\"\n\
         exit 0\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

#[test]
fn index_empty_summary_contract_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let mut summary = parse_stdout_json(&output);
    assert_index_schema_matches_output(&summary);
    normalize_index_summary(&mut summary);

    assert_json_snapshot("index_empty_summary_contract", &summary);
}

#[cfg(feature = "embeddings")]
#[test]
fn index_embeddings_feature_awaits_consent_without_download() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let summary = parse_stdout_json(&output);

    assert_eq!(summary["embeddings"]["status"], "awaiting-consent");
    assert!(summary["embeddings"]["model"].is_string());
    assert!(summary["embeddings"]["hint"].is_string());
}

#[test]
fn list_json_session_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-session")
        .project("contract-project")
        .git_branch("main")
        .user("contract question")
        .assistant("contract answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let doc = parse_stdout_json(&output);

    assert_json_schema_matches(&command_response_schema("list"), &doc, "list response");
    assert_json_snapshot("list_json_session_contract", &doc);
}

#[test]
fn search_json_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-search")
        .project("contract-project")
        .user("ordinary prompt")
        .assistant("CONTRACT_SEARCH_TOKEN answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "CONTRACT_SEARCH_TOKEN", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let mut doc = parse_stdout_json(&output);
    normalize_search_scores(&mut doc);
    assert_search_schema_matches_output(&doc);

    assert_json_snapshot("search_json_contract", &doc);
}

#[test]
fn show_json_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-show")
        .project("contract-project")
        .user("show alpha")
        .assistant("show beta")
        .user("show gamma")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "show",
            "claude-code/json-contract-show#2",
            "--format",
            "json",
            "--include-context",
            "1",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let doc = parse_stdout_json(&output);

    assert_json_schema_matches(&command_response_schema("show"), &doc, "show response");
    assert_json_snapshot("show_json_contract", &doc);
}

#[test]
fn diff_json_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-diff-a")
        .project("contract-project")
        .user("same prompt")
        .assistant("old answer")
        .done()
        .add_session("json-contract-diff-b")
        .project("contract-project")
        .user("same prompt")
        .assistant("new answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "diff",
            "claude-code/json-contract-diff-a",
            "claude-code/json-contract-diff-b",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let doc = parse_stdout_json(&output);

    assert_json_schema_matches(&command_response_schema("diff"), &doc, "diff response");
    assert_json_snapshot("diff_json_contract", &doc);
}

#[test]
fn health_json_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-health")
        .project("contract-project")
        .user("health prompt")
        .assistant("health answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("health")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let mut doc = parse_stdout_json(&output);
    assert_json_schema_matches(&command_response_schema("health"), &doc, "health response");
    assert_health_fixture_is_exercised(&doc);
    normalize_health_doc(&mut doc);

    assert_json_snapshot("health_json_contract", &doc);
}

#[cfg(unix)]
#[test]
fn sources_pull_json_contract_snapshot() {
    let workdir = tempfile::tempdir().unwrap();
    let config_path = workdir.path().join("config.toml");
    let cache_dir = workdir.path().join("sources-cache");
    let fake_rsync = write_contract_rsync(workdir.path());

    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "user@laptop.local",
            "--path",
            "/home/user/.claude",
            "--transport",
            "ssh",
            "--json",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .assert()
        .success();

    let output = aghist()
        .args(["sources", "pull", "laptop", "--json"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake_rsync)
        .assert()
        .success();
    let mut doc = parse_stdout_json(&output);
    assert_json_schema_matches(
        &command_subcommand_response_schema("sources", "pull"),
        &doc,
        "sources pull response",
    );
    normalize_sources_pull_doc(&mut doc);

    assert_json_snapshot("sources_pull_json_contract", &doc);
}

#[test]
fn mcp_tools_list_contract_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let responses = common::mcp::run_session(
        home.path(),
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        })],
    );

    assert_eq!(responses.len(), 1, "got: {responses:#?}");
    assert_json_snapshot(
        "mcp_tools_list_contract",
        &compact_mcp_tools_list_contract(&responses[0]["result"]),
    );
    assert_json_snapshot(
        "mcp_tools_output_schema_contract",
        &mcp_tools_output_schema_contract(&responses[0]["result"]),
    );
}

#[test]
fn mcp_reindex_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-mcp-reindex")
        .project("contract-project")
        .user("mcp reindex prompt")
        .assistant("mcp reindex answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let responses = common::mcp::run_session(
        home,
        &[
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "reindex",
                    "arguments": { "force": true }
                }
            }),
        ],
    );

    assert_eq!(responses.len(), 2, "got: {responses:#?}");
    let mut summary = responses[1]["result"]["structuredContent"].clone();
    assert_json_schema_matches(
        mcp_tool_output_schema(&responses[0]["result"], "reindex"),
        &summary,
        "mcp reindex structuredContent",
    );
    normalize_index_summary(&mut summary);
    assert_json_snapshot("mcp_reindex_contract", &summary);
}

#[test]
fn mcp_health_contract_snapshot() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("json-contract-mcp-health")
        .project("contract-project")
        .user("mcp health prompt")
        .assistant("mcp health answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let responses = common::mcp::run_session(
        home,
        &[
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "health",
                    "arguments": {}
                }
            }),
        ],
    );

    assert_eq!(responses.len(), 2, "got: {responses:#?}");
    let mut doc = responses[1]["result"]["structuredContent"].clone();
    assert_json_schema_matches(
        mcp_tool_output_schema(&responses[0]["result"], "health"),
        &doc,
        "mcp health structuredContent",
    );
    assert_health_fixture_is_exercised(&doc);
    normalize_health_doc(&mut doc);
    assert_json_snapshot("mcp_health_contract", &doc);
}
