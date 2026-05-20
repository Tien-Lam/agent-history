use super::*;

#[test]
fn threads_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-thread-llm")
        .project("thread-llm-proj")
        .user("thread context")
        .assistant("thread answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["threads", "--llm", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_llm_error(&output);
}

#[test]
fn decisions_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-decision-llm")
        .project("decision-llm-proj")
        .user("architecture")
        .assistant("We decided to keep SQLite instead of adding a service.")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["decisions", "--llm", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_llm_error(&output);
}

#[test]
fn todos_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-todo-llm")
        .project("todo-llm-proj")
        .user("TODO: revisit remote LLM todo refs")
        .assistant("noted")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["todos", "--llm", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_llm_error(&output);
}

#[test]
fn decisions_llm_without_api_key_returns_llm_error_envelope() {
    let mut builder = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("ses-llm-no-key")
        .project("p")
        .display("decisions session");
    builder = builder.assistant("After discussion we decided to use BM25 for ranking.");
    let fixture = builder.done().build();
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("decisions")
        .arg("--llm")
        .env("AGHIST_HOME", home)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    let stderr = assert_llm_error(&output);
    assert!(
        stderr.contains("ANTHROPIC_API_KEY"),
        "missing-key error should name the env var: {stderr:?}"
    );
}

#[test]
fn decisions_llm_model_without_llm_flag_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("decisions")
        .arg("--llm-model")
        .arg("claude-haiku-test")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"usage\""),
        "stderr should be usage envelope, got: {stderr:?}"
    );
    assert!(
        stderr.contains("--llm"),
        "hint should mention --llm: {stderr:?}"
    );
}
