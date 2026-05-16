mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use serde_json::Value;

static COMMAND_ID: AtomicUsize = AtomicUsize::new(0);

fn temp_root(label: &str) -> PathBuf {
    let id = COMMAND_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aghist-json-contract-{label}-{}-{id}",
        std::process::id()
    ))
}

fn aghist() -> Command {
    let root = temp_root("cli");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();

    let mut cmd = Command::cargo_bin("aghist").unwrap();
    cmd.env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", root.join("config.toml"));
    cmd
}

fn parse_stdout_json(output: &assert_cmd::assert::Assert) -> Value {
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON stdout, got {stdout:?}: {e}"))
}

fn assert_json_snapshot(name: &'static str, value: &Value) {
    let pretty = serde_json::to_string_pretty(value).expect("value serializes as pretty JSON");
    insta::assert_snapshot!(name, pretty);
}

fn run_mcp_session(env_home: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("aghist"))
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
    summary["duration_ms"] = serde_json::json!(0);
    summary["index_dir"] = serde_json::json!("[index-dir]");

    assert_json_snapshot("index_empty_summary_contract", &summary);
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
    assert_json_snapshot("mcp_tools_list_contract", &responses[0]["result"]);
}
