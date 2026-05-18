use super::support::*;

// ─── Hybrid (semantic) search toggle ───────────────────────────────────────────

use aghist::action::Action;

#[test]
fn hybrid_toggle_dispatch_is_inert_when_unavailable() {
    // App::new() leaves `hybrid_available` at its `false` default — we don't
    // probe the embedding pipeline until run_with_event_source. Dispatching
    // ToggleHybrid in this state must NOT flip the user-facing toggle and
    // must surface a hint message instead. Independent of the developer's
    // ~/.cache/aghist contents, so this stays deterministic in CI.
    let fixture = fixtures::claude::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));

    assert!(!app.hybrid_available());
    assert!(!app.hybrid_enabled());

    app.dispatch(Action::ToggleHybrid);
    assert!(
        !app.hybrid_enabled(),
        "toggle must not flip while unavailable"
    );
    assert_eq!(app.last_engine(), "lexical");
    assert!(
        app.status_message
            .as_deref()
            .is_some_and(|m| m.contains("unavailable")),
        "status message should explain why the toggle was a no-op, got: {:?}",
        app.status_message
    );

    // Repeated presses stay inert.
    app.dispatch(Action::ToggleHybrid);
    assert!(!app.hybrid_enabled());
}

#[test]
fn hybrid_toggle_dispatch_flips_engine_when_available() {
    // Force the pipeline into the "available" branch so we can exercise the
    // toggle without needing fastembed wired up. We never run a real query
    // here — execute_search bails when `index_ready` is false — but the
    // user-visible toggle state and the engine label are what the status
    // bar binds to, and that's what we want to lock down.
    let fixture = fixtures::claude::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    app.set_hybrid_available_for_tests(true);

    assert!(app.hybrid_available());
    assert!(
        !app.hybrid_enabled(),
        "available + tests-only setter starts OFF — run_with_event_source is what defaults it ON"
    );

    app.dispatch(Action::ToggleHybrid);
    assert!(app.hybrid_enabled(), "first press flips ON");
    assert_eq!(app.last_engine(), "hybrid");

    app.dispatch(Action::ToggleHybrid);
    assert!(!app.hybrid_enabled(), "second press flips OFF");
    assert_eq!(app.last_engine(), "lexical");
}
