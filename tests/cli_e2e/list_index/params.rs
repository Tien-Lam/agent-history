use super::super::aghist;
use super::super::common::cli;

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
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
