use super::*;
use std::fs;

use crate::model::{ContentBlock, Role};
use tempfile::TempDir;

fn write_history(dir: &Path, project: &str, body: &str) -> PathBuf {
    let project_dir = dir.join("projects").join(project);
    fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(HISTORY_FILE);
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn detect_returns_none_when_root_missing() {
    let tmp = TempDir::new().unwrap();
    let provider = AiderProvider::new(vec![tmp.path().join("nope")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn parses_single_session_with_user_and_assistant() {
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at 2026-01-15 10:30:45

> /add src/main.rs

#### How do I split this auth handler?

Extract the validator into its own function:

```rust
fn validate(t: &str) -> bool { !t.is_empty() }
```

That keeps the handler shorter.

> Tokens: 234 sent, 56 received.
";
    let history = write_history(tmp.path(), "myapp", body);
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.provider, Provider::Aider);
    assert_eq!(s.project_name.as_deref(), Some("myapp"));
    assert_eq!(s.source_path, history);
    assert!(s.summary.as_deref().unwrap().contains("split this auth"));

    let messages = provider.load_messages(s).unwrap();
    // tool (`> /add`), user, assistant, tool (token count) = 4
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, Role::Tool);
    assert_eq!(messages[1].role, Role::User);
    assert_eq!(messages[2].role, Role::Assistant);
    assert_eq!(messages[3].role, Role::Tool);

    // Assistant should contain a code block.
    let has_code = messages[2]
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::CodeBlock { .. }));
    assert!(has_code, "assistant turn should preserve code block");
}

#[test]
fn splits_multiple_sessions_in_one_file() {
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at 2026-01-15 10:30:45

#### first question

first answer.

# aider chat started at 2026-01-16 09:00:00

#### second question

second answer.
";
    write_history(tmp.path(), "repo", body);
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    // Sorted newest-first.
    assert!(sessions[0].started_at > sessions[1].started_at);
    for s in &sessions {
        let msgs = provider.load_messages(s).unwrap();
        assert_eq!(msgs.iter().filter(|m| m.role == Role::User).count(), 1);
        assert_eq!(msgs.iter().filter(|m| m.role == Role::Assistant).count(), 1);
    }
}

#[test]
fn discovers_history_in_nested_project_dir() {
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at 2026-01-15 10:30:45

#### hi

hello
";
    write_history(tmp.path(), "group/inner", body);
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].project_name.as_deref(), Some("inner"));
}

#[test]
fn skips_noise_directories() {
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at 2026-01-15 10:30:45

#### hi

hello
";
    // Drop a history file inside a node_modules subtree — it must NOT
    // be discovered, otherwise vendored fixture files would pollute the
    // session list.
    let buried = tmp
        .path()
        .join("projects")
        .join("good")
        .join("node_modules")
        .join("pkg");
    fs::create_dir_all(&buried).unwrap();
    fs::write(buried.join(HISTORY_FILE), body).unwrap();
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn ignores_corrupt_session_header() {
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at not-a-real-date

#### bogus

#### bogus too

# aider chat started at 2026-01-15 10:30:45

#### good

answer
";
    write_history(tmp.path(), "repo", body);
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "only the well-formed session should survive"
    );
}

#[test]
fn fenced_role_markers_are_not_message_boundaries() {
    // A `####` literal inside a code fence must not start a new user
    // message — otherwise we'd shred markdown headings written into
    // assistant code samples.
    let tmp = TempDir::new().unwrap();
    let body = "\
# aider chat started at 2026-01-15 10:30:45

#### show me a markdown sample

Sure:

```md
#### Subheading
> Inside fence
```

Done.
";
    write_history(tmp.path(), "repo", body);
    let provider = AiderProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    let msgs = provider.load_messages(&sessions[0]).unwrap();
    assert_eq!(msgs.iter().filter(|m| m.role == Role::User).count(), 1);
    assert_eq!(msgs.iter().filter(|m| m.role == Role::Assistant).count(), 1);
}

#[test]
fn aider_root_env_var_extends_search_paths() {
    let tmp = TempDir::new().unwrap();
    let custom = tmp.path().join("custom-root");
    fs::create_dir_all(custom.join("repo")).unwrap();
    fs::write(
        custom.join("repo").join(HISTORY_FILE),
        "# aider chat started at 2026-02-01 12:00:00\n\n#### hi\n\nhello\n",
    )
    .unwrap();

    // base_dirs() reads AIDER_ROOT; we test that path inclusion directly
    // rather than mutating the global env (test parallelism).
    let provider = AiderProvider::new(vec![custom]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
}
