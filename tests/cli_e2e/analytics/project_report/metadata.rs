use super::super::*;

#[test]
fn report_commands_respect_metadata_filters() {
    let fixture = metadata_filtered_fixture();

    let usage = aghist()
        .args(["usage", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&usage.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["totals"]["session_count"], 1);

    let project = aghist()
        .args(["project", "meta-proj", "--json", "--starred"])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(project.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&project.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["session_count"], 1);
    assert_eq!(parsed["decisions"][0]["session_id"], "session-meta-keep");
    assert_eq!(parsed["todos"][0]["session_id"], "session-meta-keep");

    let report = aghist()
        .args([
            "report",
            "--since",
            FIXTURE_SINCE,
            "--until",
            FIXTURE_UNTIL,
            "--json",
            "--starred",
        ])
        .env("AGHIST_HOME", &fixture.home)
        .env("AGHIST_METADATA_DB", &fixture.db_path)
        .output()
        .unwrap();
    assert_eq!(report.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&report.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["session_count"], 1);
    assert_eq!(parsed["decisions"][0]["session_id"], "session-meta-keep");
    assert_eq!(parsed["todos"][0]["session_id"], "session-meta-keep");
}
