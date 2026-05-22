use super::aghist;
use super::common;

#[test]
fn health_returns_ok_envelope_with_writable_index() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let assert = aghist()
        .args(["health"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let parsed = common::cli::assert_stdout_json(&assert);
    assert_eq!(parsed["ok"], true);
    let checks = parsed["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "expected at least one health check");

    // Find the providers-detected check — should be ok with the fixture.
    let providers_check = checks
        .iter()
        .find(|c| c["name"] == "providers-detected")
        .expect("providers-detected check missing");
    assert_eq!(providers_check["status"], "ok");
    let parse_check = checks
        .iter()
        .find(|c| c["name"] == "provider-parse-warnings")
        .expect("provider-parse-warnings check missing");
    assert_eq!(parse_check["status"], "ok");

    assert!(parsed["summary"]["ok_count"].as_u64().unwrap() >= 2);

    // ahist-80j.7: health surfaces a per-provider fidelity summary so
    // agents can see tool-call extraction quality without spawning a
    // separate diagnostic.
    let fidelity = parsed["provider_fidelity"]
        .as_array()
        .expect("provider_fidelity array");
    assert!(
        !fidelity.is_empty(),
        "expected fidelity row for the fixture provider"
    );
    let row = &fidelity[0];
    assert_eq!(row["provider"], "claude-code");
    assert!(row["session_count"].as_u64().unwrap() >= 1);
    assert!(row["parse"]["records_seen"].as_u64().unwrap() >= 1);
    assert!(row["tool_call_fidelity"]["empty_names"].as_u64().unwrap() == 0);
}
#[test]
fn health_warns_when_no_providers() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    // Empty home, no providers — providers-detected check should warn.
    let assert = aghist()
        .args(["health"])
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success(); // warns don't fail
    let parsed = common::cli::assert_stdout_json(&assert);
    assert_eq!(parsed["ok"], true); // ok=true while no fails
    let providers_check = parsed["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "providers-detected")
        .unwrap()
        .clone();
    assert_eq!(providers_check["status"], "warn");
}
