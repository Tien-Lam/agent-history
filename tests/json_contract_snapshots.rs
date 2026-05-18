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

fn command_schema(name: &str) -> Value {
    let output = aghist().args(["schema", name]).assert().success();
    parse_stdout_json(&output)
}

fn required_fields(schema: &Value) -> Vec<&str> {
    schema["required"]
        .as_array()
        .expect("schema required array")
        .iter()
        .map(|item| item.as_str().expect("required field name"))
        .collect()
}

fn assert_required_fields_present(schema: &Value, doc: &Value, label: &str) {
    for field in required_fields(schema) {
        assert!(
            doc.get(field).is_some(),
            "{label} is missing schema-required field {field}: {doc:#}"
        );
    }
}

fn assert_index_schema_matches_output(doc: &Value) {
    let schema = command_schema("index");
    assert_required_fields_present(&schema["response"], doc, "index response");
    assert_required_fields_present(
        &schema["response"]["properties"]["embeddings"],
        &doc["embeddings"],
        "index embeddings",
    );
}

fn assert_search_schema_matches_output(doc: &Value) {
    let schema = command_schema("search");
    let response = &schema["response"];
    assert_required_fields_present(response, doc, "search response");
    assert_eq!(response["type"], "object");
    assert_eq!(response["properties"]["hits"]["type"], "array");
    assert_eq!(response["properties"]["meta"]["type"], "object");

    assert_required_fields_present(&response["properties"]["meta"], &doc["meta"], "search meta");

    let hit_schema = &response["properties"]["hits"]["items"];
    let hits = doc["hits"].as_array().expect("search hits array");
    assert!(!hits.is_empty(), "contract fixture should produce a hit");
    assert_required_fields_present(hit_schema, &hits[0], "search hit");
}

fn run_mcp_session(env_home: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = StdCommand::new(common::helpers::aghist_bin())
        .arg("mcp")
        .env("AGHIST_HOME", env_home)
        .env("AGHIST_CONFIG", env_home.join("config.toml"))
        .env("AGHIST_INDEX_DIR", env_home.join("aghist-index"))
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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

    assert_json_snapshot("list_json_session_contract", &parse_stdout_json(&output));
}

#[test]
fn search_json_contract_snapshot() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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

    assert_json_snapshot("show_json_contract", &parse_stdout_json(&output));
}

#[test]
fn diff_json_contract_snapshot() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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

    assert_json_snapshot("diff_json_contract", &parse_stdout_json(&output));
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("json-contract-mcp-reindex")
        .project("contract-project")
        .user("mcp reindex prompt")
        .assistant("mcp reindex answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let responses = run_mcp_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "reindex",
                "arguments": { "force": true }
            }
        })],
    );

    assert_eq!(responses.len(), 1, "got: {responses:#?}");
    let mut summary = responses[0]["result"]["structuredContent"].clone();
    normalize_index_summary(&mut summary);
    assert_json_snapshot("mcp_reindex_contract", &summary);
}
