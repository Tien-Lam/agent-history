use super::aghist;
use super::common;
use predicates::prelude::*;

#[test]
fn list_with_no_data_exits_three_for_empty() {
    // Tests run under assert_cmd; stdout is piped, so --list auto-emits NDJSON.
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Empty list emits zero session rows; the trailing `{"meta": ...}` row
    // is always present so streaming consumers can detect end-of-stream.
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "empty list under NDJSON should emit zero session rows, got: {session_rows:?}"
    );
    assert!(
        stdout.contains("\"total\":0"),
        "trailing meta row must report total=0, got: {stdout:?}"
    );
}
#[test]
fn list_with_generated_claude_fixtures() {
    let fixture = common::fixtures::claude_single_session(4);
    // base_path is {tmpdir}/.claude, AGHIST_HOME should be the parent.
    // Under non-TTY (piped stdout), --list emits NDJSON: one row per session.
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("each NDJSON line must parse"))
        .filter(|v| v.get("id").is_some())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["provider"], "claude-code");
    assert_eq!(rows[0]["message_count"], 4);
}
#[test]
fn list_with_data_exits_zero() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .code(0);
}
#[test]
fn list_with_multiple_providers() {
    let claude = common::fixtures::claude_single_session(2);
    let codex = common::fixtures::codex_single_session(2);

    // Build a unified home dir with Claude and Codex fixtures
    let home_dir = tempfile::tempdir().unwrap();
    common::helpers::copy_dir_recursive(&claude.base_path, &home_dir.path().join(".claude"));
    let codex_sessions = home_dir.path().join(".codex").join("sessions");
    common::helpers::copy_dir_recursive(&codex.base_path, &codex_sessions);

    // Under non-TTY, --list emits NDJSON. Assert both providers appear.
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home_dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let providers: std::collections::HashSet<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            // Skip the trailing `{"meta": ...}` envelope row; only session
            // rows carry a `provider` field.
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| v["provider"].as_str().map(str::to_string))
        })
        .collect();
    assert!(providers.contains("claude-code"));
    assert!(providers.contains("codex-cli"));
}
#[test]
fn reindex_flag_clears_index() {
    // --reindex paired with --list against an empty home: the reindex side
    // should succeed (clear the index), and --list reports the empty exit
    // code 3 — the test asserts both signals.
    let dir = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--reindex")
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Search index cleared"));
}
#[test]
fn index_no_data_emits_zero_counts_json() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON on stdout, got {stdout:?}: {e}"));
    assert_eq!(parsed["added"], 0);
    assert_eq!(parsed["updated"], 0);
    assert_eq!(parsed["unchanged"], 0);
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["messages_indexed"], 0);
}
#[test]
fn index_idempotent_second_run_reports_unchanged() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-index-test")
        .project("idx-project")
        .user("Hello")
        .assistant("World")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // First run: should add the session
    let first = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(first.get_output().stdout.clone()).unwrap();
    let first_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        first_json["added"], 1,
        "first run should classify session as 'added'"
    );
    assert_eq!(first_json["updated"], 0);
    assert_eq!(first_json["unchanged"], 0);
    assert!(first_json["messages_indexed"].as_u64().unwrap() >= 1);

    // Second run: same fixture, should be unchanged
    let second = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(second.get_output().stdout.clone()).unwrap();
    let second_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(second_json["added"], 0);
    assert_eq!(second_json["updated"], 0);
    assert_eq!(
        second_json["unchanged"], 1,
        "second run should report 1 unchanged"
    );
    assert_eq!(second_json["messages_indexed"], 0);
}
#[test]
fn index_provider_filter_restricts_scope() {
    // Build a home with both Claude and Codex sessions; --provider claude-code should
    // index only the Claude one.
    let claude = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("claude-only")
        .project("c-proj")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let codex = common::fixtures::codex_single_session(2);

    let home_dir = tempfile::tempdir().unwrap();
    common::helpers::copy_dir_recursive(&claude.base_path, &home_dir.path().join(".claude"));
    common::helpers::copy_dir_recursive(
        &codex.base_path,
        &home_dir.path().join(".codex").join("sessions"),
    );
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--provider", "claude-code"])
        .env("AGHIST_HOME", home_dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(
        parsed["sessions_total"], 1,
        "only Claude session is in scope"
    );
    assert_eq!(parsed["added"], 1);
}
#[test]
fn index_unknown_provider_emits_usage_envelope_and_exits_two() {
    let home = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["index", "--provider", "bogus"])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("bogus"));
}
#[test]
fn list_json_emits_single_object_with_sessions_array() {
    let fixture = common::fixtures::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--list --json must emit valid JSON");
    let sessions = doc["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0]["id"].is_string());
    assert!(sessions[0]["provider"].is_string());
    assert!(sessions[0]["started_at"].is_string());
    assert_eq!(sessions[0]["message_count"], 3);
}
#[test]
fn list_ndjson_emits_one_session_per_line() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--ndjson"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("each NDJSON line must be valid JSON"))
        .collect();
    // One session row + one trailing `{"meta": ...}` envelope row.
    assert_eq!(parsed.len(), 2);
    let session = &parsed[0];
    assert!(session["id"].is_string());
    assert_eq!(session["message_count"], 2);
    // NDJSON session rows must NOT be wrapped in a `sessions` envelope.
    assert!(session.get("sessions").is_none());
    let meta_row = &parsed[1];
    assert!(
        meta_row.get("meta").is_some(),
        "last NDJSON row must be the meta envelope, got: {meta_row}"
    );
    assert_eq!(meta_row["meta"]["total"], 1);
}
#[test]
fn list_json_empty_returns_three_with_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["sessions"].as_array().unwrap().len(), 0);
}
#[test]
fn list_rejects_json_and_ndjson_together() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--json", "--ndjson"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}
#[test]
fn list_limit_caps_returned_sessions_and_emits_next_cursor() {
    let fixture = common::fixtures::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let sessions = doc["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2, "limit must cap returned rows");
    assert_eq!(doc["meta"]["total"], 5, "total reflects all matching rows");
    assert!(
        doc["meta"]["next_cursor"].is_string(),
        "next_cursor must be set when more results exist"
    );
}
#[test]
fn list_cursor_resumes_after_prior_page_and_paginates_to_completion() {
    let fixture = common::fixtures::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();

    // First page.
    let page1 = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page1.status.code(), Some(0));
    let doc1: serde_json::Value = serde_json::from_slice(&page1.stdout).unwrap();
    let cursor1 = doc1["meta"]["next_cursor"].as_str().unwrap().to_string();
    let ids1: Vec<String> = doc1["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();

    // Second page (resume).
    let page2 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor1])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page2.status.code(), Some(0));
    let doc2: serde_json::Value = serde_json::from_slice(&page2.stdout).unwrap();
    let ids2: Vec<String> = doc2["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids2.len(), 2);
    let cursor2 = doc2["meta"]["next_cursor"].as_str().unwrap().to_string();

    // Third (final) page — has the last session and no further cursor.
    let page3 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor2])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page3.status.code(), Some(0));
    let doc3: serde_json::Value = serde_json::from_slice(&page3.stdout).unwrap();
    let ids3: Vec<String> = doc3["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids3.len(), 1);
    assert!(
        doc3["meta"]["next_cursor"].is_null(),
        "final page must not advertise another cursor"
    );

    // No id appears across pages (no skips, no duplicates).
    let all_ids: Vec<&String> = ids1.iter().chain(ids2.iter()).chain(ids3.iter()).collect();
    let unique: std::collections::HashSet<&&String> = all_ids.iter().collect();
    assert_eq!(
        unique.len(),
        all_ids.len(),
        "pagination must not duplicate ids"
    );
    assert_eq!(all_ids.len(), 5, "all 5 sessions must be visited");
}
#[test]
fn list_invalid_cursor_returns_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--cursor", "not-a-real-cursor!!!"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}
#[test]
fn list_cursor_requires_list_flag() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--cursor", "abc"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}
#[test]
fn index_params_force_flag() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"force": true}).to_string();

    aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
}
#[test]
fn index_params_unknown_provider_slug_emits_usage() {
    let home = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"provider": "bogus"}).to_string();
    let assert = aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn index_help_documents_accept_download_flag() {
    aghist()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--accept-download"))
        .stdout(predicate::str::contains("AllMiniLML6V2"));
}
#[test]
fn index_summary_includes_embeddings_block() {
    // Without the `embeddings` cargo feature compiled in, the summary should
    // surface that explicitly so callers (and humans) know nothing semantic
    // happened — even when --accept-download is passed.
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--accept-download"])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON on stdout, got {stdout:?}: {e}"));
    let block = &parsed["embeddings"];
    assert!(block.is_object(), "expected embeddings object, got {block}");
    let status = block["status"].as_str().unwrap_or("");
    // The lean default build reports "disabled"; a feature build reports
    // "awaiting-consent" or "enabled". Accept any of those — the contract is
    // that the field exists and tells the caller what happened.
    assert!(
        matches!(status, "disabled" | "awaiting-consent" | "enabled"),
        "unexpected embeddings status: {status:?}"
    );
}
