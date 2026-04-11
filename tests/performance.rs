mod common;

use std::time::Instant;

use aghist::config::Config;
use aghist::provider::HistoryProvider;

use common::fixtures;

#[test]
fn session_discovery_many_sessions() {
    let (dirs, providers) = fixtures::all_generated_providers(50, 4);
    let start = Instant::now();

    let mut total = 0;
    for p in &providers {
        let sessions = p.discover_sessions().unwrap();
        total += sessions.len();
    }

    let elapsed = start.elapsed();
    println!(
        "Discovered {total} sessions across {} providers in {:?}",
        providers.len(),
        elapsed,
    );
    drop(dirs);

    assert!(total >= 250, "expected at least 250 sessions, got {total}");
    assert!(
        elapsed.as_secs() < 10,
        "discovery took too long: {elapsed:?}"
    );
}

#[test]
fn message_loading_large_session() {
    let fixture = fixtures::claude_single_session(200);
    let provider =
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]);

    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);

    let start = Instant::now();
    let messages = provider.load_messages(&sessions[0]).unwrap();
    let elapsed = start.elapsed();

    println!(
        "Loaded {} messages in {:?}",
        messages.len(),
        elapsed,
    );
    assert_eq!(messages.len(), 200);
    assert!(
        elapsed.as_secs() < 5,
        "message loading took too long: {elapsed:?}"
    );
}

#[test]
fn lru_cache_under_pressure() {
    use std::num::NonZeroUsize;
    use lru::LruCache;

    let cache_size = NonZeroUsize::new(10).unwrap();
    let mut cache: LruCache<String, Vec<String>> = LruCache::new(cache_size);

    let start = Instant::now();
    for i in 0..1000 {
        let key = format!("session-{i}");
        let msgs: Vec<String> = (0..50).map(|m| format!("message {m}")).collect();
        cache.put(key, msgs);
    }
    let elapsed = start.elapsed();

    println!("1000 cache put operations in {:?}", elapsed);
    assert_eq!(cache.len(), 10);
    assert!(
        elapsed.as_millis() < 500,
        "cache operations took too long: {elapsed:?}"
    );
}

#[test]
fn multi_provider_aggregation_and_sort() {
    let (dirs, providers) = fixtures::all_generated_providers(20, 6);

    let start = Instant::now();
    let mut all_sessions = Vec::new();
    for p in &providers {
        if let Ok(sessions) = p.discover_sessions() {
            all_sessions.extend(sessions);
        }
    }
    all_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let elapsed = start.elapsed();

    println!(
        "Aggregated and sorted {} sessions in {:?}",
        all_sessions.len(),
        elapsed,
    );
    drop(dirs);

    assert!(all_sessions.len() >= 100);
    assert!(
        elapsed.as_secs() < 5,
        "aggregation took too long: {elapsed:?}"
    );
}

#[test]
fn full_app_render_cycle() {
    use crossterm::event::KeyCode;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use common::helpers::ScriptedEventSource;

    let (dirs, providers) = fixtures::all_generated_providers(10, 8);
    let mut app = aghist::app::App::new(providers, Config::default());
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut keys = Vec::new();
    // Browse through some sessions
    for _ in 0..5 {
        keys.push(KeyCode::Char('j'));
    }
    // Select one, scroll, go back
    keys.push(KeyCode::Enter);
    for _ in 0..3 {
        keys.push(KeyCode::Char('j'));
    }
    keys.push(KeyCode::Esc);
    keys.push(KeyCode::Char('q'));

    let start = Instant::now();
    let events = ScriptedEventSource::from_keys(keys);
    app.run_with_event_source(&mut terminal, events).unwrap();
    let elapsed = start.elapsed();

    println!("Full app render cycle in {:?}", elapsed);
    drop(dirs);

    assert!(app.should_quit());
    assert!(
        elapsed.as_secs() < 10,
        "render cycle took too long: {elapsed:?}"
    );
}
