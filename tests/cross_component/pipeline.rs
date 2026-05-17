use super::support::*;

#[test]
fn claude_full_pipeline_content_blocks() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    // Walk every message and verify each ContentBlock variant is well-formed
    for msg in &messages {
        assert!(!msg.id.0.is_empty() || msg.role == Role::System);
        assert!(!msg.content.is_empty());

        for block in &msg.content {
            match block {
                ContentBlock::Text(t) | ContentBlock::Thinking(t) => {
                    assert!(!t.is_empty());
                }
                ContentBlock::CodeBlock { code, .. } => assert!(!code.is_empty()),
                ContentBlock::ToolUse(tc) => {
                    assert!(!tc.name.is_empty(), "tool call name must not be empty");
                }
                ContentBlock::ToolResult(tr) => {
                    assert!(
                        !tr.tool_call_id.is_empty(),
                        "tool result must reference a call"
                    );
                }
                ContentBlock::Error(e) => assert!(!e.is_empty()),
            }
        }
    }

    // Verify specific pipeline transformations:
    // 1. Markdown code fences in text → split into Text + CodeBlock
    let assistant_msg = &messages[3];
    let block_types: Vec<&str> = assistant_msg
        .content
        .iter()
        .map(|c| match c {
            ContentBlock::Text(_) => "text",
            ContentBlock::CodeBlock { .. } => "code",
            ContentBlock::Thinking(_) => "thinking",
            ContentBlock::ToolUse(_) => "tool_use",
            ContentBlock::ToolResult(_) => "tool_result",
            ContentBlock::Error(_) => "error",
        })
        .collect();
    assert_eq!(block_types, vec!["thinking", "text", "code", "text"]);

    // 2. Tool use input → pretty-printed JSON arguments
    let tool_msg = &messages[1];
    let tool_block = tool_msg
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::ToolUse(tc) => Some(tc),
            _ => None,
        })
        .unwrap();
    assert!(
        tool_block.arguments.contains("main.rs"),
        "tool args should contain path"
    );

    // 3. Token usage flows from raw JSON through to Message
    assert!(assistant_msg.token_usage.is_some());
}

#[test]
fn gemini_full_pipeline_content_blocks() {
    let provider = GeminiCliProvider::new(vec![fixtures_dir().join("gemini")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    // Gemini assistant message should have: text parts + code block + thinking + tool call
    let assistant = &messages[1];
    assert_eq!(assistant.role, Role::Assistant);

    let block_types: Vec<&str> = assistant
        .content
        .iter()
        .map(|c| match c {
            ContentBlock::Text(_) => "text",
            ContentBlock::CodeBlock { .. } => "code",
            ContentBlock::Thinking(_) => "thinking",
            ContentBlock::ToolUse(_) => "tool_use",
            _ => "other",
        })
        .collect();

    assert!(block_types.contains(&"text"), "should have text");
    assert!(
        block_types.contains(&"code"),
        "should have code block from markdown"
    );
    assert!(block_types.contains(&"thinking"), "should have thinking");
    assert!(block_types.contains(&"tool_use"), "should have tool call");

    // Verify token usage pipeline
    let usage = assistant.token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 30);
    assert_eq!(usage.output_tokens, 150);
}

#[test]
fn opencode_full_pipeline_code_changes() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    // Assistant message: text + diff code block from codeChanges
    let assistant = &messages[1];
    let diff_block = assistant.content.iter().find_map(|c| match c {
        ContentBlock::CodeBlock { language, code } => {
            if language.as_deref().unwrap_or("").contains("diff") {
                Some((language, code))
            } else {
                None
            }
        }
        _ => None,
    });

    let (lang, code) = diff_block.expect("should have diff block");
    assert!(
        lang.as_deref().unwrap().contains("src/db.rs"),
        "language label should contain file path"
    );
    assert!(
        code.contains("Pool::singleton"),
        "diff should contain new code"
    );
}

// ─── Multi-provider aggregation ──────────────────────────────────────────────

#[test]
fn multi_provider_discover_and_merge() {
    let providers = all_providers();
    let mut all_sessions = Vec::new();

    for p in &providers {
        let sessions = p.discover_sessions().unwrap();
        all_sessions.extend(sessions);
    }

    // All 5 providers should contribute sessions
    assert_eq!(all_sessions.len(), 5);

    let provider_types: Vec<Provider> = all_sessions.iter().map(|s| s.provider).collect();
    assert!(provider_types.contains(&Provider::ClaudeCode));
    assert!(provider_types.contains(&Provider::CopilotCli));
    assert!(provider_types.contains(&Provider::GeminiCli));
    assert!(provider_types.contains(&Provider::CodexCli));
    assert!(provider_types.contains(&Provider::OpenCode));
}

#[test]
fn multi_provider_sort_by_date() {
    let providers = all_providers();
    let mut all_sessions = Vec::new();

    for p in &providers {
        all_sessions.extend(p.discover_sessions().unwrap());
    }

    all_sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));

    // Verify sorted descending by started_at
    for window in all_sessions.windows(2) {
        assert!(
            window[0].started_at >= window[1].started_at,
            "sessions should be sorted newest-first: {:?} ({}) should be >= {:?} ({})",
            window[0].provider,
            window[0].started_at,
            window[1].provider,
            window[1].started_at,
        );
    }
}

#[test]
fn multi_provider_load_all_messages() {
    let providers = all_providers();
    let mut all_sessions = Vec::new();

    for p in &providers {
        all_sessions.extend(p.discover_sessions().unwrap());
    }

    // Load messages for every session, verify none fail
    for session in &all_sessions {
        let provider = providers
            .iter()
            .find(|p| p.provider() == session.provider)
            .unwrap();

        let messages = provider.load_messages(session).unwrap();
        assert!(
            !messages.is_empty(),
            "{:?} session {} should have messages",
            session.provider,
            session.id.0
        );

        // Every message should have non-empty content
        for msg in &messages {
            assert!(
                !msg.content.is_empty(),
                "message {} in {:?} session should have content",
                msg.id.0,
                session.provider
            );
        }
    }
}

// ─── LRU cache pattern ──────────────────────────────────────────────────────
