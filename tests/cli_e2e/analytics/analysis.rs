use super::*;

#[path = "analysis/llm.rs"]
mod llm;
#[path = "analysis/metadata.rs"]
mod metadata;
#[path = "analysis/remote.rs"]
mod remote;
#[path = "analysis/schema.rs"]
mod schema;

fn assert_llm_error(output: &std::process::Output) -> String {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
    stderr
}
