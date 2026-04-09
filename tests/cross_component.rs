use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::{fs, thread, time::Duration};

use lru::LruCache;

use aghist::model::{ContentBlock, Message, Provider, Role};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;
use aghist::search::SearchIndex;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ─── Full parse pipeline: JSONL → RawEntry → Message → ContentBlock ─────────

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
                ContentBlock::Text(t) => assert!(!t.is_empty()),
                ContentBlock::CodeBlock { code, .. } => assert!(!code.is_empty()),
                ContentBlock::ToolUse(tc) => {
                    assert!(!tc.name.is_empty(), "tool call name must not be empty");
                }
                ContentBlock::ToolResult(tr) => {
                    assert!(!tr.tool_call_id.is_empty(), "tool result must reference a call");
                }
                ContentBlock::Thinking(t) => assert!(!t.is_empty()),
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
    let tool_block = tool_msg.content.iter().find_map(|c| match c {
        ContentBlock::ToolUse(tc) => Some(tc),
        _ => None,
    }).unwrap();
    assert!(tool_block.arguments.contains("main.rs"), "tool args should contain path");

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
    assert!(block_types.contains(&"code"), "should have code block from markdown");
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
    assert!(lang.as_deref().unwrap().contains("src/db.rs"), "language label should contain file path");
    assert!(code.contains("Pool::singleton"), "diff should contain new code");
}

// ─── Multi-provider aggregation ──────────────────────────────────────────────

fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
    ]
}

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

    all_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

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

#[test]
fn cache_hit_returns_same_messages() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let session = &sessions[0];

    let mut cache: LruCache<String, Vec<Message>> =
        LruCache::new(NonZeroUsize::new(20).unwrap());

    // First load: cache miss
    assert!(!cache.contains(&session.id.0));
    let messages = provider.load_messages(session).unwrap();
    let msg_count = messages.len();
    cache.put(session.id.0.clone(), messages);

    // Second access: cache hit
    assert!(cache.contains(&session.id.0));
    let cached = cache.get(&session.id.0).unwrap();
    assert_eq!(cached.len(), msg_count);
}

#[test]
fn cache_eviction_at_capacity() {
    let mut cache: LruCache<String, Vec<Message>> =
        LruCache::new(NonZeroUsize::new(3).unwrap());

    let providers = all_providers();
    let mut all_sessions = Vec::new();
    for p in &providers {
        all_sessions.extend(p.discover_sessions().unwrap());
    }

    // Load 5 sessions into a cache with capacity 3
    for session in &all_sessions {
        let provider = providers
            .iter()
            .find(|p| p.provider() == session.provider)
            .unwrap();
        let messages = provider.load_messages(session).unwrap();
        cache.put(session.id.0.clone(), messages);
    }

    // Only 3 most recent entries should remain
    assert_eq!(cache.len(), 3);

    // First 2 sessions should have been evicted
    assert!(!cache.contains(&all_sessions[0].id.0));
    assert!(!cache.contains(&all_sessions[1].id.0));

    // Last 3 should still be present
    assert!(cache.contains(&all_sessions[2].id.0));
    assert!(cache.contains(&all_sessions[3].id.0));
    assert!(cache.contains(&all_sessions[4].id.0));
}

#[test]
fn cache_lru_access_prevents_eviction() {
    let mut cache: LruCache<String, Vec<Message>> =
        LruCache::new(NonZeroUsize::new(2).unwrap());

    let providers = all_providers();
    let mut all_sessions = Vec::new();
    for p in &providers {
        all_sessions.extend(p.discover_sessions().unwrap());
    }

    // Insert session 0 and 1
    for session in &all_sessions[..2] {
        let provider = providers
            .iter()
            .find(|p| p.provider() == session.provider)
            .unwrap();
        let messages = provider.load_messages(session).unwrap();
        cache.put(session.id.0.clone(), messages);
    }

    // Access session 0 to make it recently used
    let _ = cache.get(&all_sessions[0].id.0);

    // Insert session 2 — this should evict session 1 (LRU), not session 0
    let provider = providers
        .iter()
        .find(|p| p.provider() == all_sessions[2].provider)
        .unwrap();
    let messages = provider.load_messages(&all_sessions[2]).unwrap();
    cache.put(all_sessions[2].id.0.clone(), messages);

    assert!(cache.contains(&all_sessions[0].id.0), "recently accessed should survive");
    assert!(!cache.contains(&all_sessions[1].id.0), "LRU entry should be evicted");
    assert!(cache.contains(&all_sessions[2].id.0), "newest entry should be present");
}

// ─── Search index ───────────────────────────────────────────────────────────

#[test]
fn search_index_build_and_query() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    let stats = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats.messages_indexed > 0);

    // Search for content we know exists in Claude fixture
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty(), "should find 'build error' in Claude fixture");
    assert_eq!(hits[0].session_id, "session-abc123");
    assert!(!hits[0].snippet.is_empty());

    // Search for content in Gemini fixture
    let hits = index.search("async", 10).unwrap();
    assert!(!hits.is_empty(), "should find 'async' in Gemini fixture");

    // Search for content that doesn't exist
    let hits = index.search("xyznonexistent", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_index_incremental_rebuild() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();

    // First build indexes everything
    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);

    // Second build should skip (mtime unchanged)
    let stats2 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert_eq!(stats2.sessions_indexed, 0, "no sessions should need re-indexing");

    // Search still works after incremental rebuild
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty());
}

#[test]
fn search_index_clear_and_rebuild() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    // Clear wipes everything
    index.clear().unwrap();
    let hits = index.search("build error", 10).unwrap();
    assert!(hits.is_empty(), "should find nothing after clear");

    // Rebuild restores results
    index.build_index(&sessions, &providers, &tx).unwrap();
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty(), "should find results after rebuild");
}

#[test]
fn search_roundtrip_verifies_message_ids() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    // "build error" appears in msg-001 (user) and msg-002 (assistant) of session-abc123.
    // Tantivy parses multi-word queries as OR, so other sessions with "error" may also match.
    let hits = index.search("build error", 50).unwrap();
    assert!(!hits.is_empty());

    let claude_hits: Vec<_> = hits
        .iter()
        .filter(|h| h.session_id == "session-abc123")
        .collect();
    assert!(
        claude_hits.len() >= 2,
        "expected at least 2 hits from session-abc123, got {}",
        claude_hits.len()
    );

    let hit_message_ids: Vec<&str> = claude_hits.iter().map(|h| h.message_id.as_str()).collect();
    assert!(
        hit_message_ids.contains(&"msg-001"),
        "should find user message msg-001, got: {hit_message_ids:?}"
    );
    assert!(
        hit_message_ids.contains(&"msg-002"),
        "should find assistant message msg-002, got: {hit_message_ids:?}"
    );

    for hit in &hits {
        assert!(hit.score > 0.0, "score should be positive");
    }

    // "missing semicolon" appears in msg-004 (thinking block)
    let hits = index.search("missing semicolon", 10).unwrap();
    assert!(!hits.is_empty(), "should find 'missing semicolon' in thinking block");
    let claude_hit = hits
        .iter()
        .find(|h| h.session_id == "session-abc123")
        .expect("should have a hit from session-abc123");
    assert_eq!(claude_hit.message_id, "msg-004");
}

#[test]
fn search_empty_index() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let (tx, _rx) = crossbeam_channel::unbounded();

    // Index with zero sessions
    let stats = index.build_index(&[], &providers, &tx).unwrap();
    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.messages_indexed, 0);

    // Any query returns empty
    let hits = index.search("build error", 10).unwrap();
    assert!(hits.is_empty(), "empty index should return no hits");

    let hits = index.search("async", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_no_results_queries() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    // Various non-matching queries
    for query in &["xyznonexistent12345", "quantum_entanglement_flux", "zebra"] {
        let hits = index.search(query, 10).unwrap();
        assert!(hits.is_empty(), "query '{query}' should return no hits");
    }

    // Empty/whitespace queries
    let hits = index.search("", 10).unwrap();
    assert!(hits.is_empty(), "empty query should return no hits");

    let hits = index.search("   ", 10).unwrap();
    assert!(hits.is_empty(), "whitespace query should return no hits");
}

#[test]
fn search_incremental_reindex_after_file_change() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("index");
    let fixture_dir = tmp.path().join("claude");

    // Copy Claude fixture to a temp directory we can modify
    let src = fixtures_dir().join("claude");
    copy_dir_recursive(&src, &fixture_dir);

    let providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(ClaudeCodeProvider::new(vec![fixture_dir.clone()]))];

    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let index = SearchIndex::open_or_create(&index_dir).unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();

    // First build
    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);
    assert!(stats1.messages_indexed > 0);

    // Verify original content is indexed
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty());

    // New content should NOT be found yet
    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(hits.is_empty());

    // Modify the session file: append a new message with unique content
    let session_file = fixture_dir
        .join("projects")
        .join("test-project")
        .join("session-abc123.jsonl");

    // Ensure mtime actually changes (some filesystems have 1s resolution)
    thread::sleep(Duration::from_millis(1100));

    let new_line = r#"{"type":"user","uuid":"msg-005","timestamp":"2025-04-08T10:01:00Z","message":{"role":"user","content":"Apply the quantum entanglement refactor to the parser"},"cwd":"/home/user/project"}"#;
    let mut content = fs::read_to_string(&session_file).unwrap();
    content.push_str(new_line);
    content.push('\n');
    fs::write(&session_file, content).unwrap();

    // Re-discover sessions (mtime has changed)
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    // Incremental rebuild should re-index the changed session
    let stats2 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(
        stats2.sessions_indexed > 0,
        "changed file should be re-indexed"
    );

    // New content should now be searchable
    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "new content should appear after incremental reindex"
    );
    assert_eq!(hits[0].session_id, "session-abc123");
    assert_eq!(hits[0].message_id, "msg-005");

    // Old content should still be searchable
    let hits = index.search("build error", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "original content should survive incremental reindex"
    );
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}
