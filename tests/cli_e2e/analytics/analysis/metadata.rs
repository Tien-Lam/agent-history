use super::*;

#[test]
fn analysis_commands_respect_metadata_filters() {
    let fixture = metadata_filtered_fixture();

    let decisions = aghist()
        .args(["decisions", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(decisions.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&decisions.stdout).unwrap().trim()).unwrap();
    let rows = parsed["decisions"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|row| row["session_id"] == "session-meta-keep"));

    let todos = aghist()
        .args(["todos", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(todos.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&todos.stdout).unwrap().trim()).unwrap();
    let rows = parsed["todos"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["session_id"], "session-meta-keep");

    let threads = aghist()
        .args(["threads", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(threads.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&threads.stdout).unwrap().trim()).unwrap();
    let refs = parsed["threads"][0]["session_refs"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0], "claude-code/session-meta-keep");

    let track = aghist()
        .args(["track", "BM25 ranking", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_eq!(track.status.code(), Some(3));
}
