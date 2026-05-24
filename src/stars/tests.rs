use super::*;
use tempfile::TempDir;

#[test]
fn toggle_round_trips_through_metadata_db() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("metadata.db");

    let mut store = StarStore::load_from(&path);
    assert_eq!(store.count(), 0);
    assert!(!store.is_starred(Provider::ClaudeCode, "abc"));

    let now = store.toggle(Provider::ClaudeCode, "abc").unwrap();
    assert!(now);
    assert!(store.is_starred(Provider::ClaudeCode, "abc"));
    assert_eq!(store.count(), 1);

    let store2 = StarStore::load_from(&path);
    assert_eq!(store2.count(), 1);
    assert!(store2.is_starred(Provider::ClaudeCode, "abc"));
}

#[test]
fn toggle_off_removes_entry() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("metadata.db");

    let mut store = StarStore::load_from(&path);
    store.toggle(Provider::CodexCli, "xyz").unwrap();
    let now = store.toggle(Provider::CodexCli, "xyz").unwrap();
    assert!(!now);
    assert_eq!(store.count(), 0);

    let store2 = StarStore::load_from(&path);
    assert_eq!(store2.count(), 0);
}

#[test]
fn ephemeral_does_not_write() {
    let mut store = StarStore::ephemeral();
    store.toggle(Provider::OpenCode, "id").unwrap();
    assert!(store.is_starred(Provider::OpenCode, "id"));
}

#[test]
fn toggle_conflict_keeps_existing_persisted_timestamp() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("metadata.db");
    let conn = metadata::open(&path).unwrap();
    let star = metadata::star_add(&conn, "claude-code/abc").unwrap();
    let expected = parse_starred_at(&star.starred_at).unwrap();

    let mut store = StarStore {
        path: Some(path),
        starred: std::collections::HashMap::new(),
    };

    assert!(store.toggle(Provider::ClaudeCode, "abc").unwrap());
    assert_eq!(
        store
            .starred
            .get(&(Provider::ClaudeCode, "abc".to_string()))
            .copied(),
        Some(expected)
    );
}

#[test]
fn turn_level_stars_are_ignored_by_tui_cache() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("metadata.db");
    let conn = metadata::open(&path).unwrap();
    metadata::star_add(&conn, "claude-code/abc#7").unwrap();

    let store = StarStore::load_from(&path);
    assert_eq!(store.count(), 0);
    assert!(!store.is_starred(Provider::ClaudeCode, "abc"));
}
