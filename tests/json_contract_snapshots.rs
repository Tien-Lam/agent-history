mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};

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

fn run_mcp_session(env_home: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = StdCommand::new(common::helpers::aghist_bin())
        .arg("mcp")
        .env("AGHIST_HOME", env_home)
        .env("AGHIST_CONFIG", env_home.join("config.toml"))
        .env("AGHIST_INDEX_DIR", env_home.join("aghist-index"))
        .env("AGHIST_METADATA_DB", env_home.join("metadata.db"))
        .env("AGHIST_SOURCES_CACHE_DIR", env_home.join("sources-cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aghist mcp");

    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for req in requests {
            let line = serde_json::to_string(req).unwrap();
            stdin.write_all(line.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }

    let output = child.wait_with_output().expect("wait_with_output");
    assert!(
        output.status.success(),
        "aghist mcp exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout utf8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("response is valid JSON"))
        .collect()
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

#[test]
fn mcp_tools_list_contract_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let responses = run_mcp_session(
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

    let responses = run_mcp_session(
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

    let responses = run_mcp_session(
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
