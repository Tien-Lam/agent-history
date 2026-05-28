use super::*;
use crate::model::{ContentBlock, Role};
use parse::INDEX_FILE;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn sessions_dir(root: &Path) -> PathBuf {
    root.join(SESSIONS_SUBDIR)
}

fn write_session(root: &Path, id: &str, lines: &str) -> PathBuf {
    let sd = sessions_dir(root);
    fs::create_dir_all(&sd).unwrap();
    let path = sd.join(format!("{id}.jsonl"));
    fs::write(&path, lines).unwrap();
    path
}

fn provider_for(tmp: &TempDir) -> ContinueDevProvider {
    ContinueDevProvider::new(vec![tmp.path().to_path_buf()])
}

#[test]
fn discover_returns_empty_when_sessions_dir_missing() {
    let tmp = TempDir::new().unwrap();
    let p = provider_for(&tmp);
    assert!(p.discover_sessions().unwrap().is_empty());
}

#[test]
fn discovers_jsonl_session_files() {
    let tmp = TempDir::new().unwrap();
    write_session(tmp.path(), "abc-uuid", r#"{"role":"user","content":"hi"}"#);
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "abc-uuid");
}

#[test]
fn ignores_non_jsonl_files() {
    let tmp = TempDir::new().unwrap();
    let sd = sessions_dir(tmp.path());
    fs::create_dir_all(&sd).unwrap();
    fs::write(sd.join("index.json"), "[]").unwrap();
    fs::write(sd.join("session.txt"), "nope").unwrap();
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[cfg(unix)]
#[test]
fn ignores_symlinked_jsonl_session_files() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let sd = sessions_dir(tmp.path());
    fs::create_dir_all(&sd).unwrap();
    let target = tmp.path().join("outside.jsonl");
    fs::write(&target, r#"{"role":"user","content":"secret"}"#).unwrap();
    symlink(&target, sd.join("linked.jsonl")).unwrap();

    let sessions = provider_for(&tmp).discover_sessions().unwrap();

    assert!(sessions.is_empty());
}

#[test]
fn uses_index_title_as_summary() {
    let tmp = TempDir::new().unwrap();
    let sd = sessions_dir(tmp.path());
    fs::create_dir_all(&sd).unwrap();
    fs::write(
        sd.join(INDEX_FILE),
        r#"[{"sessionId":"my-uuid","title":"My Session","dateCreated":"2026-01-01T00:00:00Z"}]"#,
    )
    .unwrap();
    write_session(tmp.path(), "my-uuid", r#"{"role":"user","content":"hi"}"#);
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions[0].summary.as_deref(), Some("My Session"));
    // 2026-01-01T00:00:00Z
    assert_eq!(sessions[0].started_at.timestamp(), 1_767_225_600);
}

#[test]
fn tolerates_object_index_and_role_fields() {
    let tmp = TempDir::new().unwrap();
    let sd = sessions_dir(tmp.path());
    fs::create_dir_all(&sd).unwrap();
    fs::write(
        sd.join(INDEX_FILE),
        r#"[
            {"sessionId":{"id":"uuid1"},"title":{"text":"Object title"},"dateCreated":{"timestamp":"2026-01-01T00:00:00Z"}},
            "skip malformed index entry"
        ]"#,
    )
    .unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        r#"{"role":{"role":"user"},"content":{"text":"object content"}}
{"role":{"type":"assistant"},"content":"reply"}"#,
    );

    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].summary.as_deref(), Some("Object title"));
    assert_eq!(sessions[0].started_at.timestamp(), 1_767_225_600);
    assert_eq!(sessions[0].message_count, 2);

    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, Role::User);
    assert!(matches!(
        &msgs[0].content[0],
        ContentBlock::Text(text) if text == "object content"
    ));
    assert_eq!(msgs[1].role, Role::Assistant);
}

#[test]
fn parses_string_content() {
    let tmp = TempDir::new().unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        "{\"role\":\"user\",\"content\":\"hello\"}\n{\"role\":\"assistant\",\"content\":\"hi\"}",
    );
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, Role::User);
    assert_eq!(msgs[1].role, Role::Assistant);
}

#[test]
fn parses_block_content() {
    let tmp = TempDir::new().unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        r#"{"role":"user","content":[{"type":"text","text":"block text"}]}"#,
    );
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 1);
    assert!(matches!(&msgs[0].content[0], ContentBlock::Text(_)));
}

#[test]
fn parses_tool_use_and_tool_result_blocks() {
    let tmp = TempDir::new().unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        r#"{"role":"assistant","content":[{"type":"tool_use","id":"tu1","name":"read_file","input":{"path":"x.rs"}}]}
{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu1","content":[{"type":"text","text":"ok"},{"type":"image","text":"ignored"}]}]}"#,
    );
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 2);
    assert!(
        matches!(&msgs[0].content[0], ContentBlock::ToolUse(tc) if tc.name == "read_file" && tc.arguments.contains("\"path\""))
    );
    assert!(
        matches!(&msgs[1].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "tu1" && tr.output == "ok")
    );
}

#[test]
fn skips_unknown_roles() {
    let tmp = TempDir::new().unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        "{\"role\":\"system\",\"content\":\"sys\"}\n{\"role\":\"user\",\"content\":\"hi\"}",
    );
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    // system is now included (Role::System)
    assert_eq!(msgs.len(), 2);
}

#[test]
fn load_skips_corrupt_jsonl_lines() {
    let tmp = TempDir::new().unwrap();
    write_session(
        tmp.path(),
        "uuid1",
        "not json\n{\"role\":\"unknown\",\"content\":\"skip\"}\n{\"role\":\"assistant\",\"content\":\"\"}\n{\"role\":\"user\",\"content\":\"kept\"}",
    );
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    assert_eq!(sessions[0].message_count, 1);
    let load = p.load_messages_with_stats(&sessions[0]).unwrap();
    assert_eq!(load.messages.len(), 1);
    assert!(matches!(&load.messages[0].content[0], ContentBlock::Text(text) if text == "kept"));
    assert_eq!(load.parse_stats.records_seen, 4);
    assert_eq!(load.parse_stats.parse_errors, 1);
    assert_eq!(load.parse_stats.skipped_records, 1);
    assert_eq!(load.parse_stats.empty_content, 1);
}
