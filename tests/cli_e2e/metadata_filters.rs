use super::common;

#[path = "metadata_filters/list.rs"]
mod list;
#[path = "metadata_filters/notes.rs"]
mod notes;
#[path = "metadata_filters/schema.rs"]
mod schema;
#[path = "metadata_filters/search.rs"]
mod search;

fn three_session_fixture() -> (common::fixtures::core::FixtureDir, std::path::PathBuf) {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("sess-alpha")
        .project("alpha-proj")
        .user("alpha body")
        .done()
        .add_session("sess-beta")
        .project("beta-proj")
        .user("beta body")
        .done()
        .add_session("sess-gamma")
        .project("gamma-proj")
        .user("gamma body")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap().to_path_buf();
    (fixture, home)
}

fn list_session_ids_json(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(String::from))
        .collect()
}
