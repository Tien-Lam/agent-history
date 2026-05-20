use super::super::aghist;
use super::super::common;

#[test]
fn list_filter_provider_drops_other_providers() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let kept = aghist()
        .args(["--list", "--ndjson", "--provider", "claude-code"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));
    let stdout = String::from_utf8(kept.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.len() == 1,
        "expected one row for claude-code, got: {session_rows:?}"
    );

    let dropped = aghist()
        .args(["--list", "--ndjson", "--provider", "codex-cli"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(dropped.status.code(), Some(3));
    let stdout = String::from_utf8(dropped.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "expected zero rows for codex-cli, got: {session_rows:?}"
    );
}

#[test]
fn list_json_provider_round_trips_through_cli_input() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let json_out = aghist()
        .args(["--list", "--json", "--limit", "1"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(json_out.status.code(), Some(0));
    let stdout = String::from_utf8(json_out.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let provider_slug = parsed["sessions"][0]["provider"].as_str().unwrap();
    assert_eq!(provider_slug, "claude-code");

    let round_trip = aghist()
        .args(["--list", "--provider", provider_slug])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(
        round_trip.status.code(),
        Some(0),
        "round-trip failed: stderr={}",
        String::from_utf8_lossy(&round_trip.stderr)
    );
}

#[test]
fn list_filter_since_excludes_older_sessions() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--since", "2025-06-01T00:00:00Z"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "since cutoff should drop older session, got: {session_rows:?}"
    );
}

#[test]
fn list_filter_until_includes_older_sessions() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--until", "2030-01-01T00:00:00Z"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert_eq!(session_rows.len(), 1);
}

#[test]
fn list_filter_project_substring_match_is_case_insensitive() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-proj-test")
        .project("MyCoolProject")
        .user("hi")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let hit = aghist()
        .args(["--list", "--ndjson", "--project", "coolproj"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(0));

    let miss = aghist()
        .args(["--list", "--ndjson", "--project", "nope"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(3));
}

#[test]
fn list_filter_role_drops_sessions_without_matching_messages() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-role-test")
        .project("role-test")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let kept = aghist()
        .args(["--list", "--ndjson", "--role", "user"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));

    let dropped = aghist()
        .args(["--list", "--ndjson", "--role", "tool"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(dropped.status.code(), Some(3));
}

#[test]
fn list_filter_has_tool_call_keeps_only_sessions_with_tool_use() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-with-tool")
        .project("tool-yes")
        .user("run a thing")
        .assistant_with_tool("running", "Bash", r#"{"command":"ls"}"#)
        .done()
        .add_session("session-no-tool")
        .project("tool-no")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v.get("id").is_some())
        .collect();
    assert_eq!(rows.len(), 1, "expected only the tool-using session");
    assert_eq!(rows[0]["id"], "session-with-tool");
}
