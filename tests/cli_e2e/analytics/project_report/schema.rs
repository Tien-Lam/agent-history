use super::super::*;

#[test]
fn schema_subcommand_includes_project() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "project"));

    let project_schema = aghist().args(["schema", "project"]).output().unwrap();
    assert_eq!(project_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&project_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "project");
    let props = &parsed["params"]["properties"];
    assert_eq!(props["name"]["minLength"], 1);
    assert_eq!(
        props["name"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::FILTER_PROJECT_MAX_BYTES)
    );
    assert_eq!(
        props["decisions"]["maximum"],
        serde_json::json!(aghist::schema_fragments::REPORT_SECTION_LIMIT_MAX)
    );
    assert_eq!(
        props["todos"]["maximum"],
        serde_json::json!(aghist::schema_fragments::REPORT_SECTION_LIMIT_MAX)
    );
    let response = &parsed["response"]["properties"];
    assert!(response["session_count"].is_object());
    assert!(response["top_files"].is_object());
    assert!(response["time_of_day"].is_object());
    assert_eq!(response["time_of_day"]["minItems"], 24);
}

#[test]
fn schema_subcommand_includes_report() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "report"));

    let report_schema = aghist().args(["schema", "report"]).output().unwrap();
    assert_eq!(report_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&report_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "report");
    let props = &parsed["params"]["properties"];
    assert_eq!(props["days"]["default"], 7);
    assert_eq!(
        props["days"]["maximum"],
        serde_json::json!(aghist::schema_fragments::REPORT_DAYS_MAX)
    );
    assert_eq!(props["top_projects"]["default"], 3);
    assert_eq!(
        props["top_projects"]["maximum"],
        serde_json::json!(aghist::schema_fragments::REPORT_SECTION_LIMIT_MAX)
    );
    let resp = &parsed["response"]["properties"];
    assert!(resp["window"].is_object());
    assert!(resp["top_projects"].is_object());
    assert!(resp["project_count"].is_object());
}

#[test]
fn project_rejects_oversized_name() {
    let oversized = "p".repeat(aghist::schema_fragments::FILTER_PROJECT_MAX_BYTES + 1);
    let assert = aghist()
        .args(["project", oversized.as_str()])
        .assert()
        .code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("project <name> must be at most"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn report_rejects_zero_days_flag() {
    let assert = aghist().args(["report", "--days", "0"]).assert().code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("report days must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn project_rejects_zero_section_limit_flag() {
    let assert = aghist()
        .args(["project", "alpha", "--todos", "0"])
        .assert()
        .code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("section limit must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn report_rejects_zero_section_limit_flag() {
    let assert = aghist().args(["report", "--todos", "0"]).assert().code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("section limit must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}
