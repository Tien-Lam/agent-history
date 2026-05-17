use super::support::*;

#[test]
fn cache_hit_returns_same_messages() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let session = &sessions[0];

    let mut cache: LruCache<String, Vec<Message>> = LruCache::new(NonZeroUsize::new(20).unwrap());

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
    let mut cache: LruCache<String, Vec<Message>> = LruCache::new(NonZeroUsize::new(3).unwrap());

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
    let mut cache: LruCache<String, Vec<Message>> = LruCache::new(NonZeroUsize::new(2).unwrap());

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

    assert!(
        cache.contains(&all_sessions[0].id.0),
        "recently accessed should survive"
    );
    assert!(
        !cache.contains(&all_sessions[1].id.0),
        "LRU entry should be evicted"
    );
    assert!(
        cache.contains(&all_sessions[2].id.0),
        "newest entry should be present"
    );
}

// ─── Search index ───────────────────────────────────────────────────────────
