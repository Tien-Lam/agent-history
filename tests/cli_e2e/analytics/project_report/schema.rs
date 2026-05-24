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
    assert_eq!(parsed["params"]["properties"]["name"]["minLength"], 1);
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
    assert_eq!(props["top_projects"]["default"], 3);
    let resp = &parsed["response"]["properties"];
    assert!(resp["window"].is_object());
    assert!(resp["top_projects"].is_object());
    assert!(resp["project_count"].is_object());
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
