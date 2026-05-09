//! User-annotation sidecar database.
//!
//! aghist must never modify a provider's session files (Claude Code's JSONL,
//! Copilot's logs, etc). Per-user annotations — notes, tags, stars — live in a
//! separate sqlite database keyed by stable citation refs of the form
//! `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
//!
//! Default location is `~/.local/share/aghist/metadata.db` (`XDG_DATA_HOME` on
//! Linux, `Library/Application Support` on macOS, `%APPDATA%` on Windows).
//! Override with `AGHIST_METADATA_DB`.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use thiserror::Error;

use crate::model::Provider;

const ENV_PATH: &str = "AGHIST_METADATA_DB";

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("could not resolve metadata.db path; set {ENV_PATH} or ensure XDG/home dirs exist")]
    NoPath,
    #[error("create parent directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("open {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("apply migrations on {path}: {source}")]
    Migrate {
        path: PathBuf,
        #[source]
        source: rusqlite_migration::Error,
    },
    #[error("invalid session ref '{0}': {1}")]
    InvalidSessionRef(String, &'static str),
    #[error("note body must not be empty")]
    EmptyBody,
    #[error("note id {0} not found")]
    NoteNotFound(i64),
    #[error("tag must not be empty")]
    EmptyTag,
    #[error("tag '{tag}' is already attached to {session_ref}")]
    TagAlreadyExists { session_ref: String, tag: String },
    #[error("tag '{tag}' is not attached to {session_ref}")]
    TagNotFound { session_ref: String, tag: String },
    #[error("{session_ref} is already starred")]
    StarAlreadyExists { session_ref: String },
    #[error("{session_ref} is not starred")]
    StarNotFound { session_ref: String },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, MetadataError>;

/// Resolve the metadata.db path. Honors `AGHIST_METADATA_DB`, otherwise falls
/// back to the platform data dir (`~/.local/share/aghist/metadata.db` on Linux).
pub fn default_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(ENV_PATH) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::ProjectDirs::from("", "", "aghist")
        .map(|dirs| dirs.data_dir().join("metadata.db"))
}

/// Schema migrations. Append new migrations; never edit or reorder existing
/// entries — `rusqlite_migration` tracks progress via `SQLite`'s `user_version`.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("metadata/0001_init.sql"))])
}

/// Open the metadata database at `path`, creating parent directories and
/// applying any pending migrations. Idempotent: safe to call on every startup.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| MetadataError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let mut conn = Connection::open(path).map_err(|source| MetadataError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|source| MetadataError::Migrate {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(conn)
}

/// Open the metadata database at the resolved default path.
pub fn open_default() -> Result<Connection> {
    let path = default_path().ok_or(MetadataError::NoPath)?;
    open(&path)
}

/// One row from the `notes` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub id: i64,
    pub session_ref: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Validate a `session_ref` string used as a note key. Accepts
/// `<provider-slug>/<session-id>` (session-level) or
/// `<provider-slug>/<session-id>#<turn>` (turn-level). The provider slug must
/// match a known [`Provider`]; the session id must be non-empty; if a turn is
/// present it must parse as a positive integer.
pub fn validate_session_ref(raw: &str) -> std::result::Result<&str, MetadataError> {
    let invalid = |reason: &'static str| MetadataError::InvalidSessionRef(raw.to_string(), reason);
    if raw.is_empty() {
        return Err(invalid("empty"));
    }
    let (head, turn_opt) = match raw.rsplit_once('#') {
        Some((h, t)) => (h, Some(t)),
        None => (raw, None),
    };
    let (provider_slug, session_id) = head
        .split_once('/')
        .ok_or_else(|| invalid("expected '<provider>/<session-id>[#<turn>]'"))?;
    if Provider::from_slug(provider_slug).is_none() {
        return Err(invalid("unknown provider slug"));
    }
    if session_id.is_empty() {
        return Err(invalid("empty session id"));
    }
    if let Some(turn) = turn_opt {
        let n: u32 = turn.parse().map_err(|_| invalid("turn must be a positive integer"))?;
        if n == 0 {
            return Err(invalid("turn must be a positive integer"));
        }
    }
    Ok(raw)
}

fn row_to_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        session_ref: row.get(1)?,
        body: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

const NOTE_COLUMNS: &str = "id, session_ref, body, created_at, updated_at";

/// Insert a note for `session_ref` with `body`. Returns the freshly-inserted
/// row (including server-generated id and timestamps). Both the ref and body
/// are validated; empty bodies are rejected.
pub fn note_add(conn: &Connection, session_ref: &str, body: &str) -> Result<Note> {
    let session_ref = validate_session_ref(session_ref)?;
    let body = body.trim();
    if body.is_empty() {
        return Err(MetadataError::EmptyBody);
    }
    conn.execute(
        "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
        params![session_ref, body],
    )?;
    let id = conn.last_insert_rowid();
    note_get(conn, id)?.ok_or(MetadataError::NoteNotFound(id))
}

/// Fetch a single note by id. Returns `Ok(None)` for a non-existent id.
pub fn note_get(conn: &Connection, id: i64) -> Result<Option<Note>> {
    let sql = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1");
    let note = conn
        .query_row(&sql, params![id], row_to_note)
        .optional()?;
    Ok(note)
}

/// List notes, optionally filtered by `session_ref`.
///
/// Filter semantics:
/// - `None` → all notes, newest first.
/// - `Some("<provider>/<session>#<turn>")` → exact match on that turn.
/// - `Some("<provider>/<session>")` → notes on the session itself OR on any
///   of its turns (i.e. `session_ref = X` OR `session_ref LIKE 'X#%'`).
pub fn note_list(conn: &Connection, filter: Option<&str>) -> Result<Vec<Note>> {
    let order = "ORDER BY datetime(created_at) DESC, id DESC";
    let columns = NOTE_COLUMNS;
    let notes = match filter {
        None => {
            let sql = format!("SELECT {columns} FROM notes {order}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], row_to_note)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        Some(raw) => {
            validate_session_ref(raw)?;
            if raw.contains('#') {
                let sql = format!("SELECT {columns} FROM notes WHERE session_ref = ?1 {order}");
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![raw], row_to_note)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            } else {
                let prefix = format!("{raw}#%");
                let sql = format!(
                    "SELECT {columns} FROM notes \
                     WHERE session_ref = ?1 OR session_ref LIKE ?2 {order}"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![raw, prefix], row_to_note)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
        }
    };
    Ok(notes)
}

/// Replace the body of an existing note and bump `updated_at`. Returns the
/// updated row, or `MetadataError::NoteNotFound` if no row matches `id`.
pub fn note_edit(conn: &Connection, id: i64, body: &str) -> Result<Note> {
    let body = body.trim();
    if body.is_empty() {
        return Err(MetadataError::EmptyBody);
    }
    let changed = conn.execute(
        "UPDATE notes \
            SET body = ?1, \
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
          WHERE id = ?2",
        params![body, id],
    )?;
    if changed == 0 {
        return Err(MetadataError::NoteNotFound(id));
    }
    note_get(conn, id)?.ok_or(MetadataError::NoteNotFound(id))
}

/// Delete a note by id. Returns the deleted row, or
/// `MetadataError::NoteNotFound` if no row matches `id`.
pub fn note_remove(conn: &Connection, id: i64) -> Result<Note> {
    let existing = note_get(conn, id)?.ok_or(MetadataError::NoteNotFound(id))?;
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    Ok(existing)
}

/// One row from the `tags` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    pub id: i64,
    pub session_ref: String,
    pub tag: String,
    pub created_at: String,
}

const TAG_COLUMNS: &str = "id, session_ref, tag, created_at";

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        session_ref: row.get(1)?,
        tag: row.get(2)?,
        created_at: row.get(3)?,
    })
}

/// Trim and validate a tag value. Tags are user-supplied labels; aghist only
/// requires that they be non-empty after trimming. Uniqueness per
/// `session_ref` is enforced by the schema.
fn normalize_tag(tag: &str) -> Result<&str> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err(MetadataError::EmptyTag);
    }
    Ok(trimmed)
}

/// Attach `tag` to `session_ref`. Returns the freshly-inserted row. Errors with
/// `TagAlreadyExists` if the (`session_ref`, `tag`) pair is already present.
pub fn tag_add(conn: &Connection, session_ref: &str, tag: &str) -> Result<Tag> {
    let session_ref = validate_session_ref(session_ref)?;
    let tag = normalize_tag(tag)?;
    let result = conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        params![session_ref, tag],
    );
    match result {
        Ok(_) => {
            let id = conn.last_insert_rowid();
            tag_get_by_id(conn, id)?.ok_or_else(|| MetadataError::TagNotFound {
                session_ref: session_ref.to_string(),
                tag: tag.to_string(),
            })
        }
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(MetadataError::TagAlreadyExists {
                session_ref: session_ref.to_string(),
                tag: tag.to_string(),
            })
        }
        Err(e) => Err(MetadataError::Sqlite(e)),
    }
}

fn tag_get_by_id(conn: &Connection, id: i64) -> Result<Option<Tag>> {
    let sql = format!("SELECT {TAG_COLUMNS} FROM tags WHERE id = ?1");
    let tag = conn
        .query_row(&sql, params![id], row_to_tag)
        .optional()?;
    Ok(tag)
}

/// List tags, optionally filtered by `session_ref` and/or `tag` value.
///
/// Filter semantics for `session_ref`:
/// - `None` → match any `session_ref`.
/// - `Some("<provider>/<session>#<turn>")` → exact match on that turn.
/// - `Some("<provider>/<session>")` → tags on the session itself OR on any of
///   its turns.
///
/// `tag_filter` matches the tag value exactly when `Some`. Combining the two
/// narrows the result to rows that satisfy both constraints. Results are
/// ordered newest-first.
pub fn tag_list(
    conn: &Connection,
    session_ref_filter: Option<&str>,
    tag_filter: Option<&str>,
) -> Result<Vec<Tag>> {
    let order = "ORDER BY datetime(created_at) DESC, id DESC";
    let columns = TAG_COLUMNS;

    let mut clauses: Vec<&str> = Vec::new();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(raw) = session_ref_filter {
        validate_session_ref(raw)?;
        if raw.contains('#') {
            clauses.push("session_ref = ?");
            params_vec.push(Box::new(raw.to_string()));
        } else {
            clauses.push("(session_ref = ? OR session_ref LIKE ?)");
            params_vec.push(Box::new(raw.to_string()));
            params_vec.push(Box::new(format!("{raw}#%")));
        }
    }
    if let Some(raw) = tag_filter {
        let normalized = normalize_tag(raw)?;
        clauses.push("tag = ?");
        params_vec.push(Box::new(normalized.to_string()));
    }

    let sql = if clauses.is_empty() {
        format!("SELECT {columns} FROM tags {order}")
    } else {
        format!("SELECT {columns} FROM tags WHERE {} {order}", clauses.join(" AND "))
    };

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(AsRef::as_ref).collect();
    let rows = stmt.query_map(param_refs.as_slice(), row_to_tag)?;
    let tags = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tags)
}

/// Detach `tag` from `session_ref`. Returns the deleted row, or
/// `TagNotFound` if no matching row exists.
pub fn tag_remove(conn: &Connection, session_ref: &str, tag: &str) -> Result<Tag> {
    let session_ref = validate_session_ref(session_ref)?;
    let tag = normalize_tag(tag)?;
    let sql = format!("SELECT {TAG_COLUMNS} FROM tags WHERE session_ref = ?1 AND tag = ?2");
    let existing: Option<Tag> = conn
        .query_row(&sql, params![session_ref, tag], row_to_tag)
        .optional()?;
    let existing = existing.ok_or_else(|| MetadataError::TagNotFound {
        session_ref: session_ref.to_string(),
        tag: tag.to_string(),
    })?;
    conn.execute(
        "DELETE FROM tags WHERE session_ref = ?1 AND tag = ?2",
        params![session_ref, tag],
    )?;
    Ok(existing)
}

/// One row from the `stars` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Star {
    pub session_ref: String,
    pub starred_at: String,
}

const STAR_COLUMNS: &str = "session_ref, starred_at";

fn row_to_star(row: &rusqlite::Row<'_>) -> rusqlite::Result<Star> {
    Ok(Star {
        session_ref: row.get(0)?,
        starred_at: row.get(1)?,
    })
}

/// Mark `session_ref` as starred. Errors with `StarAlreadyExists` if the ref is
/// already starred. Returns the freshly-inserted row.
pub fn star_add(conn: &Connection, session_ref: &str) -> Result<Star> {
    let session_ref = validate_session_ref(session_ref)?;
    let result = conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        params![session_ref],
    );
    match result {
        Ok(_) => star_get(conn, session_ref)?.ok_or_else(|| MetadataError::StarNotFound {
            session_ref: session_ref.to_string(),
        }),
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(MetadataError::StarAlreadyExists {
                session_ref: session_ref.to_string(),
            })
        }
        Err(e) => Err(MetadataError::Sqlite(e)),
    }
}

/// Fetch a single starred row by `session_ref`. Returns `Ok(None)` if the ref
/// is not starred.
pub fn star_get(conn: &Connection, session_ref: &str) -> Result<Option<Star>> {
    let sql = format!("SELECT {STAR_COLUMNS} FROM stars WHERE session_ref = ?1");
    let star = conn
        .query_row(&sql, params![session_ref], row_to_star)
        .optional()?;
    Ok(star)
}

/// List starred rows, optionally filtered by `session_ref`.
///
/// Filter semantics mirror notes/tags:
/// - `None` → all stars, newest first.
/// - `Some("<provider>/<session>#<turn>")` → exact match on that turn.
/// - `Some("<provider>/<session>")` → the session row OR any of its turns.
pub fn star_list(conn: &Connection, filter: Option<&str>) -> Result<Vec<Star>> {
    let order = "ORDER BY datetime(starred_at) DESC, session_ref DESC";
    let columns = STAR_COLUMNS;
    let stars = match filter {
        None => {
            let sql = format!("SELECT {columns} FROM stars {order}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], row_to_star)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        Some(raw) => {
            validate_session_ref(raw)?;
            if raw.contains('#') {
                let sql =
                    format!("SELECT {columns} FROM stars WHERE session_ref = ?1 {order}");
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![raw], row_to_star)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            } else {
                let prefix = format!("{raw}#%");
                let sql = format!(
                    "SELECT {columns} FROM stars \
                     WHERE session_ref = ?1 OR session_ref LIKE ?2 {order}"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![raw, prefix], row_to_star)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
        }
    };
    Ok(stars)
}

/// Unstar `session_ref`. Returns the deleted row, or `StarNotFound` if the ref
/// was not starred.
pub fn star_remove(conn: &Connection, session_ref: &str) -> Result<Star> {
    let session_ref = validate_session_ref(session_ref)?;
    let existing = star_get(conn, session_ref)?.ok_or_else(|| MetadataError::StarNotFound {
        session_ref: session_ref.to_string(),
    })?;
    conn.execute("DELETE FROM stars WHERE session_ref = ?1", params![session_ref])?;
    Ok(existing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
        assert!(dup.is_err(), "duplicate (session_ref,tag) should violate UNIQUE");

        // Same session_ref with a different tag is allowed.
        conn.execute(
            "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
            ("claude-code/abc", "todo"),
        )
        .unwrap();
    }

    fn open_fresh() -> (TempDir, Connection) {
        let tmp = TempDir::new().unwrap();
        let conn = open(&tmp.path().join("metadata.db")).unwrap();
        (tmp, conn)
    }

    #[test]
    fn validate_accepts_session_and_turn_refs() {
        validate_session_ref("claude-code/abc-123").unwrap();
        validate_session_ref("claude-code/abc-123#7").unwrap();
        validate_session_ref("opencode/ses_xyz#99").unwrap();
    }

    #[test]
    fn validate_rejects_bad_refs() {
        assert!(matches!(
            validate_session_ref(""),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            validate_session_ref("no-slash"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            validate_session_ref("Claude-Code/abc"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            validate_session_ref("claude-code/"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            validate_session_ref("claude-code/abc#0"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            validate_session_ref("claude-code/abc#two"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
    }

    #[test]
    fn note_add_returns_populated_row() {
        let (_tmp, conn) = open_fresh();
        let note = note_add(&conn, "claude-code/abc-123#7", "first note body").unwrap();
        assert!(note.id >= 1);
        assert_eq!(note.session_ref, "claude-code/abc-123#7");
        assert_eq!(note.body, "first note body");
        assert!(!note.created_at.is_empty());
        assert_eq!(note.created_at, note.updated_at);
    }

    #[test]
    fn note_add_rejects_invalid_ref_and_empty_body() {
        let (_tmp, conn) = open_fresh();
        assert!(matches!(
            note_add(&conn, "bad-provider/abc", "body"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            note_add(&conn, "claude-code/abc", "   \n  "),
            Err(MetadataError::EmptyBody)
        ));
    }

    #[test]
    fn note_add_trims_body() {
        let (_tmp, conn) = open_fresh();
        let note = note_add(&conn, "claude-code/abc", "  hello  \n").unwrap();
        assert_eq!(note.body, "hello");
    }

    #[test]
    fn note_list_filters_by_session_or_turn() {
        let (_tmp, conn) = open_fresh();
        let session = note_add(&conn, "claude-code/abc", "session-level").unwrap();
        let turn7 = note_add(&conn, "claude-code/abc#7", "turn 7").unwrap();
        let turn9 = note_add(&conn, "claude-code/abc#9", "turn 9").unwrap();
        let other = note_add(&conn, "opencode/xyz", "different session").unwrap();

        let all = note_list(&conn, None).unwrap();
        assert_eq!(all.len(), 4);

        // Session-level filter sees the session row + every turn under it,
        // but not unrelated sessions.
        let scoped = note_list(&conn, Some("claude-code/abc")).unwrap();
        let ids: Vec<_> = scoped.iter().map(|n| n.id).collect();
        assert!(ids.contains(&session.id));
        assert!(ids.contains(&turn7.id));
        assert!(ids.contains(&turn9.id));
        assert!(!ids.contains(&other.id));

        // Turn-level filter is exact: only that turn, not the parent session.
        let turn_only = note_list(&conn, Some("claude-code/abc#7")).unwrap();
        assert_eq!(turn_only.len(), 1);
        assert_eq!(turn_only[0].id, turn7.id);
    }

    #[test]
    fn note_edit_updates_body_and_bumps_timestamp() {
        let (_tmp, conn) = open_fresh();
        let original = note_add(&conn, "claude-code/abc", "v1").unwrap();
        // Force a measurable gap so updated_at moves even on fast machines.
        std::thread::sleep(std::time::Duration::from_millis(10));
        let updated = note_edit(&conn, original.id, "v2").unwrap();
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.body, "v2");
        assert_eq!(updated.created_at, original.created_at);
        assert!(
            updated.updated_at >= original.updated_at,
            "updated_at should advance: {} -> {}",
            original.updated_at,
            updated.updated_at
        );
    }

    #[test]
    fn note_edit_rejects_missing_id_and_empty_body() {
        let (_tmp, conn) = open_fresh();
        let note = note_add(&conn, "claude-code/abc", "v1").unwrap();
        assert!(matches!(
            note_edit(&conn, 9999, "v2"),
            Err(MetadataError::NoteNotFound(9999))
        ));
        assert!(matches!(
            note_edit(&conn, note.id, "  "),
            Err(MetadataError::EmptyBody)
        ));
    }

    #[test]
    fn note_remove_returns_deleted_row_and_is_idempotent_negative() {
        let (_tmp, conn) = open_fresh();
        let note = note_add(&conn, "claude-code/abc", "to remove").unwrap();
        let removed = note_remove(&conn, note.id).unwrap();
        assert_eq!(removed, note);
        assert!(note_get(&conn, note.id).unwrap().is_none());
        assert!(matches!(
            note_remove(&conn, note.id),
            Err(MetadataError::NoteNotFound(_))
        ));
    }

    #[test]
    fn tag_add_returns_populated_row() {
        let (_tmp, conn) = open_fresh();
        let tag = tag_add(&conn, "claude-code/abc", "review").unwrap();
        assert!(tag.id >= 1);
        assert_eq!(tag.session_ref, "claude-code/abc");
        assert_eq!(tag.tag, "review");
        assert!(!tag.created_at.is_empty());
    }

    #[test]
    fn tag_add_trims_and_rejects_empty() {
        let (_tmp, conn) = open_fresh();
        let tag = tag_add(&conn, "claude-code/abc", "  todo  ").unwrap();
        assert_eq!(tag.tag, "todo");
        assert!(matches!(
            tag_add(&conn, "claude-code/abc", "   "),
            Err(MetadataError::EmptyTag)
        ));
    }

    #[test]
    fn tag_add_rejects_duplicate() {
        let (_tmp, conn) = open_fresh();
        tag_add(&conn, "claude-code/abc", "review").unwrap();
        let err = tag_add(&conn, "claude-code/abc", "review");
        assert!(matches!(
            err,
            Err(MetadataError::TagAlreadyExists { ref session_ref, ref tag })
                if session_ref == "claude-code/abc" && tag == "review"
        ));
    }

    #[test]
    fn tag_add_rejects_invalid_ref() {
        let (_tmp, conn) = open_fresh();
        assert!(matches!(
            tag_add(&conn, "bad-provider/abc", "review"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
    }

    #[test]
    fn tag_list_filters_by_session_or_turn_or_tag_value() {
        let (_tmp, conn) = open_fresh();
        let session = tag_add(&conn, "claude-code/abc", "review").unwrap();
        let turn7 = tag_add(&conn, "claude-code/abc#7", "todo").unwrap();
        let turn7_review = tag_add(&conn, "claude-code/abc#7", "review").unwrap();
        let other = tag_add(&conn, "opencode/xyz", "review").unwrap();

        let all = tag_list(&conn, None, None).unwrap();
        assert_eq!(all.len(), 4);

        // Session-level filter sees the session row + every turn under it.
        let scoped = tag_list(&conn, Some("claude-code/abc"), None).unwrap();
        let ids: Vec<_> = scoped.iter().map(|t| t.id).collect();
        assert!(ids.contains(&session.id));
        assert!(ids.contains(&turn7.id));
        assert!(ids.contains(&turn7_review.id));
        assert!(!ids.contains(&other.id));

        // Turn-level filter is exact.
        let turn_only = tag_list(&conn, Some("claude-code/abc#7"), None).unwrap();
        let ids: Vec<_> = turn_only.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&turn7.id));
        assert!(ids.contains(&turn7_review.id));

        // Tag-value filter narrows across sessions.
        let reviews = tag_list(&conn, None, Some("review")).unwrap();
        assert_eq!(reviews.len(), 3);

        // Combined filters AND together.
        let scoped_review = tag_list(&conn, Some("claude-code/abc"), Some("review")).unwrap();
        let ids: Vec<_> = scoped_review.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&session.id));
        assert!(ids.contains(&turn7_review.id));
    }

    #[test]
    fn tag_remove_returns_deleted_row_and_is_idempotent_negative() {
        let (_tmp, conn) = open_fresh();
        let added = tag_add(&conn, "claude-code/abc", "review").unwrap();
        let removed = tag_remove(&conn, "claude-code/abc", "review").unwrap();
        assert_eq!(removed, added);

        // Removing the same pair again yields TagNotFound.
        assert!(matches!(
            tag_remove(&conn, "claude-code/abc", "review"),
            Err(MetadataError::TagNotFound { .. })
        ));
    }

    #[test]
    fn tag_remove_rejects_invalid_ref_and_empty_tag() {
        let (_tmp, conn) = open_fresh();
        assert!(matches!(
            tag_remove(&conn, "bad/abc", "review"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
        assert!(matches!(
            tag_remove(&conn, "claude-code/abc", "  "),
            Err(MetadataError::EmptyTag)
        ));
    }

    #[test]
    fn star_add_returns_populated_row() {
        let (_tmp, conn) = open_fresh();
        let star = star_add(&conn, "claude-code/abc-123").unwrap();
        assert_eq!(star.session_ref, "claude-code/abc-123");
        assert!(!star.starred_at.is_empty());
    }

    #[test]
    fn star_add_rejects_duplicate() {
        let (_tmp, conn) = open_fresh();
        star_add(&conn, "claude-code/abc").unwrap();
        let err = star_add(&conn, "claude-code/abc");
        assert!(matches!(
            err,
            Err(MetadataError::StarAlreadyExists { ref session_ref })
                if session_ref == "claude-code/abc"
        ));
    }

    #[test]
    fn star_add_rejects_invalid_ref() {
        let (_tmp, conn) = open_fresh();
        assert!(matches!(
            star_add(&conn, "bad-provider/abc"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
    }

    #[test]
    fn star_list_filters_by_session_or_turn() {
        let (_tmp, conn) = open_fresh();
        let session = star_add(&conn, "claude-code/abc").unwrap();
        let turn7 = star_add(&conn, "claude-code/abc#7").unwrap();
        let other = star_add(&conn, "opencode/xyz").unwrap();

        let all = star_list(&conn, None).unwrap();
        assert_eq!(all.len(), 3);

        let scoped = star_list(&conn, Some("claude-code/abc")).unwrap();
        let refs: Vec<_> = scoped.iter().map(|s| s.session_ref.clone()).collect();
        assert!(refs.contains(&session.session_ref));
        assert!(refs.contains(&turn7.session_ref));
        assert!(!refs.contains(&other.session_ref));

        let turn_only = star_list(&conn, Some("claude-code/abc#7")).unwrap();
        assert_eq!(turn_only.len(), 1);
        assert_eq!(turn_only[0].session_ref, "claude-code/abc#7");
    }

    #[test]
    fn star_remove_returns_deleted_row_and_is_idempotent_negative() {
        let (_tmp, conn) = open_fresh();
        let added = star_add(&conn, "claude-code/abc").unwrap();
        let removed = star_remove(&conn, "claude-code/abc").unwrap();
        assert_eq!(removed, added);
        assert!(star_get(&conn, "claude-code/abc").unwrap().is_none());

        assert!(matches!(
            star_remove(&conn, "claude-code/abc"),
            Err(MetadataError::StarNotFound { .. })
        ));
    }

    #[test]
    fn star_remove_rejects_invalid_ref() {
        let (_tmp, conn) = open_fresh();
        assert!(matches!(
            star_remove(&conn, "bad/abc"),
            Err(MetadataError::InvalidSessionRef(_, _))
        ));
    }

    #[test]
    fn env_var_overrides_default_path() {
        let tmp = TempDir::new().unwrap();
        let custom = tmp.path().join("custom.db");
        // SAFETY: tests run sequentially within a test binary by default; the
        // env var is set and read here only.
        std::env::set_var(ENV_PATH, &custom);
        let resolved = default_path().expect("path resolves with env var set");
        std::env::remove_var(ENV_PATH);
        assert_eq!(resolved, custom);
    }
}
