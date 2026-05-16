use std::fmt::Write as _;
use std::fs;

use tempfile::TempDir;

use super::claude::ClaudeFixtureBuilder;
use super::codex::CodexFixtureBuilder;
use super::copilot::CopilotFixtureBuilder;
use super::cursor::CursorFixtureBuilder;
use super::gemini::GeminiFixtureBuilder;
use super::opencode::OpenCodeFixtureBuilder;

pub fn all_generated_providers(
    n_sessions: usize,
    msgs_per_session: usize,
) -> (
    Vec<TempDir>,
    Vec<Box<dyn aghist::provider::HistoryProvider>>,
) {
    vec![
        generated_claude_provider(n_sessions, msgs_per_session),
        generated_copilot_provider(n_sessions, msgs_per_session),
        generated_gemini_provider(n_sessions, msgs_per_session),
        generated_codex_provider(n_sessions, msgs_per_session),
        generated_opencode_provider(n_sessions, msgs_per_session),
        generated_cursor_provider(n_sessions, msgs_per_session),
        generated_aider_provider(n_sessions, msgs_per_session),
        generated_zed_ai_provider(n_sessions, msgs_per_session),
        generated_cline_provider(n_sessions, msgs_per_session),
        generated_continue_dev_provider(n_sessions, msgs_per_session),
    ]
    .into_iter()
    .unzip()
}

type GeneratedProvider = (TempDir, Box<dyn aghist::provider::HistoryProvider>);

fn generated_claude_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::claude_code::ClaudeCodeProvider;

    let mut claude = ClaudeFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = claude
            .add_session(&format!("session-{s:03}"))
            .project(&format!("project-{s}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m} in session {s}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m} in session {s}"));
            }
        }
        claude = sb.done();
    }

    let cf = claude.build();
    (
        cf.dir,
        Box::new(ClaudeCodeProvider::new(vec![cf.base_path.clone()])),
    )
}

fn generated_copilot_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::copilot_cli::CopilotCliProvider;

    let mut copilot = CopilotFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = copilot.add_session(&format!("copilot-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        copilot = sb.done();
    }

    let cpf = copilot.build();
    (
        cpf.dir,
        Box::new(CopilotCliProvider::new(vec![cpf.base_path.clone()])),
    )
}

fn generated_gemini_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::gemini_cli::GeminiCliProvider;

    let mut gemini = GeminiFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = gemini.add_session(&format!("gemini-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.gemini(&format!("Gemini msg {m}"));
            }
        }
        gemini = sb.done();
    }

    let gf = gemini.build();
    (
        gf.dir,
        Box::new(GeminiCliProvider::new(vec![gf.base_path.clone()])),
    )
}

fn generated_codex_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::codex_cli::CodexCliProvider;

    let mut codex = CodexFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = codex.add_session(&format!("codex-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        codex = sb.done();
    }

    let cxf = codex.build();
    (
        cxf.dir,
        Box::new(CodexCliProvider::new(vec![cxf.base_path.clone()])),
    )
}

fn generated_opencode_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::opencode::OpenCodeProvider;

    let mut opencode = OpenCodeFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = opencode.add_session(&format!("oc-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        opencode = sb.done();
    }

    let ocf = opencode.build();
    (
        ocf.dir,
        Box::new(OpenCodeProvider::new(vec![ocf.base_path.clone()])),
    )
}

fn generated_cursor_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::cursor::CursorProvider;

    let mut cursor = CursorFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = cursor.add_session(&format!("comp-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        cursor = sb.done();
    }

    let crf = cursor.build();
    (
        crf.dir,
        Box::new(CursorProvider::new(vec![crf.base_path.clone()])),
    )
}

fn generated_aider_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::aider::AiderProvider;

    let dir = TempDir::new().unwrap();
    let base = dir.path().to_path_buf();
    let projects = base.join("projects");
    fs::create_dir_all(&projects).unwrap();

    for s in 0..n_sessions {
        let project_dir = projects.join(format!("aider-project-{s:03}"));
        fs::create_dir_all(&project_dir).unwrap();

        let mut body = format!("# aider chat started at 2026-01-01 00:{s:02}:00\n\n");
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                writeln!(body, "#### User msg {m} in session {s}").unwrap();
            } else {
                writeln!(body, "Assistant msg {m} in session {s}").unwrap();
            }
        }

        fs::write(project_dir.join(".aider.chat.history.md"), body).unwrap();
    }

    (dir, Box::new(AiderProvider::new(vec![base])))
}

fn generated_zed_ai_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::zed_ai::ZedAiProvider;

    let dir = TempDir::new().unwrap();
    let conversations = dir.path().join("conversations");
    fs::create_dir_all(&conversations).unwrap();

    for s in 0..n_sessions {
        let messages: Vec<serde_json::Value> = (0..msgs_per_session)
            .map(|m| {
                let role = if m % 2 == 0 { "user" } else { "assistant" };
                serde_json::json!({
                    "id": format!("zed-{s:03}-{m:03}"),
                    "role": role,
                    "text": format!("{role} msg {m} in session {s}"),
                    "timestamp": format!("2026-01-01T00:{s:02}:{m:02}Z"),
                })
            })
            .collect();

        let json = serde_json::json!({
            "id": format!("zed-conv-{s:03}"),
            "summary": format!("Zed session {s}"),
            "model": "zed-test-model",
            "workspace": format!("/tmp/zed-project-{s:03}"),
            "created_at": format!("2026-01-01T00:{s:02}:00Z"),
            "updated_at": format!("2026-01-01T00:{s:02}:59Z"),
            "messages": messages,
        });
        fs::write(
            conversations.join(format!("zed-conv-{s:03}.json")),
            serde_json::to_vec_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    let base = dir.path().to_path_buf();
    (dir, Box::new(ZedAiProvider::new(vec![base])))
}

fn generated_cline_provider(n_sessions: usize, msgs_per_session: usize) -> GeneratedProvider {
    use aghist::provider::cline::ClineProvider;

    let dir = TempDir::new().unwrap();
    let tasks = dir.path().join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&tasks).unwrap();

    for s in 0..n_sessions {
        let task_id = (1_767_225_600_000_i64 + i64::try_from(s).unwrap_or(0)).to_string();
        let task_dir = tasks.join(&task_id);
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(
            task_dir.join("task_metadata.json"),
            format!(
                r#"{{"createdAt":{}}}"#,
                1_767_225_600_000_i64 + i64::try_from(s).unwrap_or(0)
            ),
        )
        .unwrap();
        fs::write(
            task_dir.join("ui_messages.json"),
            format!(r#"[{{"type":"say","say":"task","text":"Cline session {s}"}}]"#),
        )
        .unwrap();

        let messages: Vec<serde_json::Value> = (0..msgs_per_session)
            .map(|m| {
                let role = if m % 2 == 0 { "user" } else { "assistant" };
                serde_json::json!({
                    "role": role,
                    "content": format!("{role} msg {m} in session {s}"),
                })
            })
            .collect();
        fs::write(
            task_dir.join("api_conversation_history.json"),
            serde_json::to_vec_pretty(&messages).unwrap(),
        )
        .unwrap();
    }

    let base = dir.path().to_path_buf();
    (dir, Box::new(ClineProvider::new(vec![base])))
}

fn generated_continue_dev_provider(
    n_sessions: usize,
    msgs_per_session: usize,
) -> GeneratedProvider {
    use aghist::provider::continue_dev::ContinueDevProvider;

    let dir = TempDir::new().unwrap();
    let sessions = dir.path().join("sessions");
    fs::create_dir_all(&sessions).unwrap();

    let mut index = Vec::new();
    for s in 0..n_sessions {
        let session_id = format!("continue-{s:03}");
        index.push(serde_json::json!({
            "sessionId": session_id,
            "title": format!("Continue session {s}"),
            "dateCreated": format!("2026-01-01T00:{s:02}:00Z"),
        }));

        let lines: Vec<String> = (0..msgs_per_session)
            .map(|m| {
                let role = if m % 2 == 0 { "user" } else { "assistant" };
                serde_json::json!({
                    "role": role,
                    "content": format!("{role} msg {m} in session {s}"),
                })
                .to_string()
            })
            .collect();
        fs::write(
            sessions.join(format!("{session_id}.jsonl")),
            format!("{}\n", lines.join("\n")),
        )
        .unwrap();
    }
    fs::write(
        sessions.join("index.json"),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();

    let base = dir.path().to_path_buf();
    (dir, Box::new(ContinueDevProvider::new(vec![base])))
}
