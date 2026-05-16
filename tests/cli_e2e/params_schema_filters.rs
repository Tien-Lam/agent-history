use super::aghist;
use super::common;
use predicates::prelude::*;
use std::collections::BTreeSet;

#[test]
fn schema_list_emits_subcommand_index() {
    let assert = aghist().args(["schema", "--list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let names = parsed["subcommands"].as_array().expect("subcommands array");
    assert!(names.iter().any(|n| n == "search"));
    assert!(names.iter().any(|n| n == "schema"));
}
#[test]
fn schema_for_search_is_valid_json_schema() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(parsed["command"], "search");
    assert!(parsed["params"]["properties"]["query"].is_object());
    assert!(parsed["response"].is_object());
    assert!(parsed["exit_codes"]["0"].is_string());
}
#[test]
fn schema_for_search_documents_filter_flags() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    for name in [
        "provider",
        "since",
        "until",
        "project",
        "role",
        "has_tool_call",
    ] {
        assert!(
            props[name].is_object(),
            "search schema missing filter param: {name}"
        );
    }
    assert_eq!(props["provider"]["type"], "string");
    assert_eq!(props["since"]["format"], "date-time");
    assert_eq!(
        props["role"]["enum"],
        serde_json::json!(["user", "assistant", "tool"])
    );
    assert_eq!(props["has_tool_call"]["type"], "boolean");
}
#[test]
fn schema_for_list_documents_filter_flags() {
    let assert = aghist().args(["schema", "list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    for name in [
        "provider",
        "since",
        "until",
        "project",
        "role",
        "has_tool_call",
    ] {
        assert!(
            props[name].is_object(),
            "list schema missing filter param: {name}"
        );
    }
}
#[test]
fn schema_all_dumps_every_subcommand() {
    let index = aghist().args(["schema", "--list"]).assert().success();
    let index_stdout = String::from_utf8(index.get_output().stdout.clone()).unwrap();
    let index: serde_json::Value = serde_json::from_str(index_stdout.trim()).unwrap();
    let listed: BTreeSet<&str> = index["subcommands"]
        .as_array()
        .expect("subcommands array")
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();

    let all = aghist().args(["schema", "--all"]).assert().success();
    let all_stdout = String::from_utf8(all.get_output().stdout.clone()).unwrap();
    let all: serde_json::Value = serde_json::from_str(all_stdout.trim()).unwrap();
    let map = all.as_object().expect("top-level object");
    let dumped: BTreeSet<&str> = map.keys().map(String::as_str).collect();
    assert_eq!(dumped, listed);

    for name in listed {
        assert!(map.contains_key(name), "missing schema for {name}");
        assert_eq!(map[name]["$id"], format!("aghist:schema/{name}"));
    }
}
#[test]
fn schema_unknown_subcommand_exits_one_with_envelope() {
    let output = aghist().args(["schema", "nonsense"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.trim().lines().last().unwrap()).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonsense"));
}
#[test]
fn schema_without_args_exits_two_usage() {
    let output = aghist().arg("schema").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// ─── Filters: --provider --since --until --project --role --has-tool-call ───
//
// These flags apply to both `--list` and `search`. They're declared as global
// on the top-level `Cli` so users can place them either before or after the
// subcommand name.
#[test]
fn filters_help_lists_all_six_flags() {
    aghist()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--provider <SLUG>"))
        .stdout(predicate::str::contains("--since <RFC3339>"))
        .stdout(predicate::str::contains("--until <RFC3339>"))
        .stdout(predicate::str::contains("--project <NAME>"))
        .stdout(predicate::str::contains("--role <ROLE>"))
        .stdout(predicate::str::contains("--has-tool-call"));
}
#[test]
fn filter_provider_rejects_unknown_slug() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--provider", "not-a-provider"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unknown provider slug"),
        "expected provider validation error, got: {stderr}"
    );
}
#[test]
fn filter_role_rejects_unknown_value() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--role", "robot"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unknown role"),
        "expected role validation error, got: {stderr}"
    );
}
#[test]
fn filter_since_rejects_non_rfc3339() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--since", "yesterday"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("RFC 3339"),
        "expected RFC 3339 validation error, got: {stderr}"
    );
}
#[test]
fn list_filter_provider_drops_other_providers() {
    let fixture = common::fixtures::claude_single_session(2);
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
    // No codex sessions in fixture → EXIT_EMPTY (3) with zero session rows
    // (the trailing `{"meta":...}` envelope row is still emitted).
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
    // Regression: ahist-jqb. JSON output used to emit "claude_code"
    // (snake_case) while --provider only accepts "claude-code" (kebab),
    // breaking `aghist --list --json | jq -r .sessions[0].provider |
    // xargs aghist --provider`. Output now matches the input slug.
    let fixture = common::fixtures::claude_single_session(2);
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

    // Feed the slug back through --provider — must not error.
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
    // Default fixture session is at 2025-01-01T00:00:00Z. Picking a since
    // strictly after that should drop it.
    let fixture = common::fixtures::claude_single_session(2);
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
    let fixture = common::fixtures::claude_single_session(2);
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    // Session has only user/assistant messages; --role tool should drop it.
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    // Session A: tool-use; Session B: text only.
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
#[test]
fn search_filter_provider_pushes_into_index_query() {
    // Single-provider fixture; --provider matching should keep results,
    // --provider mismatching should empty them.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let kept = aghist()
        .args(["search", "User", "--json", "--provider", "claude-code"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let dropped = aghist()
        .args(["search", "User", "--json", "--provider", "codex-cli"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    // No codex-cli docs match → EXIT_EMPTY.
    assert_eq!(dropped.status.code(), Some(3));
}
#[test]
fn search_filter_role_restricts_hits_to_matching_role() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-role")
        .project("rolesearch")
        .user("apple banana")
        .assistant("cherry banana")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let user_only = aghist()
        .args(["search", "banana", "--json", "--role", "user"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(user_only.status.code(), Some(0));
    let stdout = String::from_utf8(user_only.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    assert_eq!(rows.len(), 1, "user-role filter should leave one hit");
    let snippet = rows[0]["snippet"].as_str().unwrap_or("");
    assert!(
        snippet.contains("apple"),
        "expected user message in hit, got snippet: {snippet}"
    );
}
#[test]
fn search_filter_has_tool_call_keeps_only_tool_messages() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-tool")
        .project("toolsearch")
        .user("search-keyword run me")
        .assistant_with_tool("ok", "Bash", r#"{"command":"search-keyword"}"#)
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "search-keyword", "--json", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    // Both messages contain "search-keyword", but only the assistant one has
    // a tool invocation — has-tool-call should drop the user message.
    assert_eq!(rows.len(), 1, "expected only the tool-using assistant hit");
}
#[test]
fn search_filter_since_drops_old_messages() {
    // Fixture timestamps are at 2025-01-01T00:00:00–05Z. A future since cuts everything.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--json",
            "--since",
            "2030-01-01T00:00:00Z",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}
#[test]
fn search_filter_project_substring_match_is_case_insensitive() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-proj")
        .project("AwesomeProject")
        .user("findme keyword")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let hit = aghist()
        .args(["search", "findme", "--json", "--project", "awesome"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let miss = aghist()
        .args(["search", "findme", "--json", "--project", "nomatch"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(3));
}

// ─── note add/list/edit/remove ─────────────────────────────────────────────
