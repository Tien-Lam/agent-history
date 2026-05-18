use super::*;
use crate::model::{ContentBlock, Role};
use parse::{METADATA_FILE, UI_MESSAGES_FILE};
use std::fs;
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

fn make_task(root: &Path, task_id: &str, api_json: &str) -> PathBuf {
    let task = root.join(EXTENSION_ID).join(TASKS_SUBDIR).join(task_id);
    fs::create_dir_all(&task).unwrap();
    write_file(&task, API_HISTORY_FILE, api_json);
    task
}

fn provider_for(tmp: &TempDir) -> ClineProvider {
    ClineProvider::new(vec![tmp.path().to_path_buf()])
}

#[test]
fn detect_returns_none_when_dir_missing() {
    let tmp = TempDir::new().unwrap();
    let p = ClineProvider::new(vec![tmp.path().to_path_buf()]);
    assert!(p.discover_sessions().unwrap().is_empty());
}

#[test]
fn discovers_session_from_task_dir() {
    let tmp = TempDir::new().unwrap();
    make_task(tmp.path(), "1698765432000", "[]");
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "1698765432000");
}

#[test]
fn parses_timestamp_from_task_dir_name() {
    let tmp = TempDir::new().unwrap();
    make_task(tmp.path(), "1698765432000", "[]");
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions[0].started_at.timestamp(), 1_698_765_432);
}

#[test]
fn uses_task_metadata_created_at_over_dir_name() {
    let tmp = TempDir::new().unwrap();
    let task_path = tmp
        .path()
        .join(EXTENSION_ID)
        .join(TASKS_SUBDIR)
        .join("1698765432000");
    fs::create_dir_all(&task_path).unwrap();
    write_file(&task_path, API_HISTORY_FILE, "[]");
    write_file(&task_path, METADATA_FILE, "{\"createdAt\":1700000000000}");
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions[0].started_at.timestamp(), 1_700_000_000);
}

#[test]
fn extracts_summary_from_ui_messages() {
    let tmp = TempDir::new().unwrap();
    let task_path = tmp.path().join(EXTENSION_ID).join(TASKS_SUBDIR).join("123");
    fs::create_dir_all(&task_path).unwrap();
    write_file(&task_path, API_HISTORY_FILE, "[]");
    write_file(
        &task_path,
        UI_MESSAGES_FILE,
        "[{\"type\":\"say\",\"say\":\"task\",\"text\":\"Implement feature X\"}]",
    );
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert_eq!(sessions[0].summary.as_deref(), Some("Implement feature X"));
}

#[test]
fn skips_unknown_roles() {
    let tmp = TempDir::new().unwrap();
    let api = r#"[
            {"role":"system","content":[{"type":"text","text":"sys"}]},
            {"role":"user","content":[{"type":"text","text":"hello"}]}
        ]"#;
    make_task(tmp.path(), "1698765432000", api);
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, Role::User);
}

#[test]
fn parses_tool_use_and_tool_result_blocks() {
    let tmp = TempDir::new().unwrap();
    let api = r#"[
            {"role":"assistant","content":[
                {"type":"tool_use","id":"tu1","name":"write_file","input":{"path":"x.rs"}}
            ]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"tu1","content":"ok"}
            ]}
        ]"#;
    make_task(tmp.path(), "1698765432000", api);
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 2);
    assert!(matches!(&msgs[0].content[0], ContentBlock::ToolUse(tc) if tc.name == "write_file"));
    assert!(
        matches!(&msgs[1].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "tu1")
    );
}

#[test]
fn load_messages_errors_on_corrupt_json() {
    let tmp = TempDir::new().unwrap();
    make_task(tmp.path(), "1698765432000", "not json");
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    assert!(p.load_messages(&sessions[0]).is_err());
}

#[test]
fn skips_directory_without_api_history() {
    let tmp = TempDir::new().unwrap();
    let task = tmp
        .path()
        .join(EXTENSION_ID)
        .join(TASKS_SUBDIR)
        .join("1698765432000");
    fs::create_dir_all(&task).unwrap();
    let sessions = provider_for(&tmp).discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn string_content_parsed_as_text() {
    let tmp = TempDir::new().unwrap();
    let api = r#"[{"role":"user","content":"plain string content"}]"#;
    make_task(tmp.path(), "1698765432000", api);
    let p = provider_for(&tmp);
    let sessions = p.discover_sessions().unwrap();
    let msgs = p.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.len(), 1);
    assert!(matches!(&msgs[0].content[0], ContentBlock::Text(_)));
}
