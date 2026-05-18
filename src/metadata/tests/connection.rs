use super::*;

#[test]
fn migrations_validate() {
    migrations().validate().expect("migrations are valid");
}

#[test]
fn open_creates_db_and_parent_dir() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nested/dir/metadata.db");
    let conn = open(&path).unwrap();
    assert!(path.exists());
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|n| !n.starts_with("sqlite_"))
        .collect();
    assert!(tables.contains(&"notes".to_string()), "tables = {tables:?}");
    assert!(tables.contains(&"tags".to_string()), "tables = {tables:?}");
    assert!(tables.contains(&"stars".to_string()), "tables = {tables:?}");
}

#[test]
fn open_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let _ = open(&path).unwrap();
    let conn = open(&path).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert!(version >= 1, "user_version should be set after migration");
}

#[test]
fn schema_supports_basic_inserts() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
        ("claude-code/abc123#42", "first note"),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc123", "review"),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc123"],
    )
    .unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn stars_are_unique_per_session_ref() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc"],
    )
    .unwrap();
    let dup = conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc"],
    );
    assert!(dup.is_err(), "duplicate star should violate UNIQUE");
}

#[test]
fn tags_are_unique_per_session_ref_tag_pair() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "review"),
    )
    .unwrap();
    let dup = conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "review"),
    );
    assert!(
        dup.is_err(),
        "duplicate (session_ref,tag) should violate UNIQUE"
    );

    // Same session_ref with a different tag is allowed.
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "todo"),
    )
    .unwrap();
}
