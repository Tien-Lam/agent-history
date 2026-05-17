use super::*;

#[test]
fn threads_include_remote_source_refs_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-thread")
        .project("thread-proj")
        .user("thread context")
        .assistant("thread answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["threads", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let threads = parsed["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(
        threads[0]["session_refs"][0],
        "laptop:claude-code/remote-thread"
    );
    assert_eq!(threads[0]["providers"][0], "claude-code");
}

#[test]
fn threads_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
}

#[test]
fn track_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-track")
        .project("track-proj")
        .user("BM25 ranking context")
        .assistant("We changed BM25 ranking to prefer recent sessions.")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["track", "BM25 ranking", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
}

#[test]
fn decisions_include_remote_source_refs_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-decision")
        .project("decision-proj")
        .user("architecture")
        .assistant("We decided to keep SQLite instead of adding a service.")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["decisions", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let decisions = parsed["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["source"], "laptop");
    assert_eq!(decisions[0]["ref"], "laptop:claude-code/remote-decision#2");
}

#[test]
fn decisions_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
}

#[test]
fn todos_include_remote_source_refs_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-todo")
        .project("todo-proj")
        .user("TODO: revisit remote sync retries")
        .assistant("noted")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["todos", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let todos = parsed["todos"].as_array().unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0]["source"], "laptop");
    assert_eq!(todos[0]["ref"], "laptop:claude-code/remote-todo#1");
}

#[test]
fn todos_llm_finds_remote_source_candidates_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
}

#[test]
fn schema_subcommand_includes_todos_llm_shape() {
    let todos_schema = aghist().args(["schema", "todos"]).output().unwrap();
    assert_eq!(todos_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&todos_schema.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(props["llm_model"].is_object());
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2);
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["todos"]["items"]["properties"];
    for field in ["ref", "source", "description", "status_inferred"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}

#[test]
fn schema_subcommand_includes_threads_llm_shape() {
    let threads_schema = aghist().args(["schema", "threads"]).output().unwrap();
    assert_eq!(threads_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&threads_schema.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(props["llm_model"].is_object());
    assert_eq!(props["llm_max_sessions"]["default"], 200);
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2);
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["threads"]["items"]["properties"];
    for field in ["topic_summary", "member_refs", "time_span"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}

#[test]
fn schema_subcommand_includes_track() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "track"));

    let track_schema = aghist().args(["schema", "track"]).output().unwrap();
    assert_eq!(track_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&track_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "track");
    assert_eq!(parsed["params"]["properties"]["topic"]["minLength"], 1);
    let item_props = &parsed["response"]["properties"]["timeline"]["items"]["properties"];
    assert!(item_props["session_ref"].is_object());
    assert_eq!(item_props["direction"]["enum"].as_array().unwrap().len(), 4);
}

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

#[test]
fn decisions_llm_without_api_key_returns_llm_error_envelope() {
    // Generate any fixture so the heuristic emits at least one candidate.
    // We use a session with explicit decision language; the LLM path then
    // groups by session and tries to call the API — but with no key set it
    // must fail fast with kind=llm-error.
    let mut builder = common::fixtures::ClaudeFixtureBuilder::new()
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
        // Defensively unset both keys — even on a CI host that has them set
        // for other tools, this test must exercise the missing-key path.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
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
#[test]
fn decisions_schema_documents_llm_params_and_response() {
    let out = aghist().args(["schema", "decisions"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert!(props["llm"].is_object(), "llm param should be in schema");
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(
        props["llm_model"].is_object(),
        "llm_model param should be in schema"
    );
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2, "response should oneOf {{heuristic, llm}}");
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["decisions"]["items"]["properties"];
    for field in ["summary", "rationale", "alternatives", "ref", "source"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}
