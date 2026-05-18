use std::path::PathBuf;

use super::aghist;
use super::common;

/// Fixture timestamps are pinned to 2025-01-01; bracket that day so the
/// report's window includes them regardless of when the test is run.
const FIXTURE_SINCE: &str = "2024-12-25T00:00:00Z";
const FIXTURE_UNTIL: &str = "2025-01-02T00:00:00Z";

struct MetadataFilteredFixture {
    _fixture: common::fixtures::core::FixtureDir,
    home: PathBuf,
    _db_dir: tempfile::TempDir,
    db_path: PathBuf,
}

fn metadata_filtered_fixture() -> MetadataFilteredFixture {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-meta-keep")
        .project("meta-proj")
        .user("TODO: keep metadata filtered work")
        .assistant("We decided to keep SQLite instead of adding a service.")
        .done()
        .add_session("session-meta-drop")
        .project("meta-proj")
        .user("TODO: drop metadata filtered work with BM25 ranking")
        .assistant("We decided to use Postgres instead of SQLite.")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap().to_path_buf();
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("metadata.db");

    aghist()
        .args(["star", "claude-code/session-meta-keep"])
        .env("AGHIST_METADATA_DB", &db_path)
        .assert()
        .success();

    MetadataFilteredFixture {
        _fixture: fixture,
        home,
        _db_dir: db_dir,
        db_path,
    }
}

mod analysis;
mod project_report;
mod usage;
