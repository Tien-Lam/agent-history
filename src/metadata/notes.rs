use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES;

use super::{refs::SessionRefPredicate, validate_session_ref, MetadataError, Result};

/// One row from the `notes` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub id: i64,
    pub session_ref: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
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

fn normalize_note_body(body: &str) -> Result<&str> {
    let body = body.trim();
    if body.is_empty() {
        return Err(MetadataError::EmptyBody);
    }
    if body.len() > METADATA_NOTE_BODY_MAX_BYTES {
        return Err(MetadataError::BodyTooLong {
            bytes: body.len(),
            max_bytes: METADATA_NOTE_BODY_MAX_BYTES,
        });
    }
    Ok(body)
}

/// Insert a note for `session_ref` with `body`. Returns the freshly-inserted
/// row (including server-generated id and timestamps). Both the ref and body
/// are validated; empty and oversized bodies are rejected.
pub fn note_add(conn: &Connection, session_ref: &str, body: &str) -> Result<Note> {
    let session_ref = validate_session_ref(session_ref)?;
    let body = normalize_note_body(body)?;
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
    let note = conn.query_row(&sql, params![id], row_to_note).optional()?;
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
            let predicate = SessionRefPredicate::parse(raw)?;
            let sql = format!(
                "SELECT {columns} FROM notes WHERE {} {order}",
                predicate.clause()
            );
            let mut stmt = conn.prepare(&sql)?;
            let params_vec = predicate.params();
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(AsRef::as_ref).collect();
            let rows = stmt.query_map(param_refs.as_slice(), row_to_note)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    Ok(notes)
}

/// Replace the body of an existing note and bump `updated_at`. Returns the
/// updated row, or `MetadataError::NoteNotFound` if no row matches `id`.
pub fn note_edit(conn: &Connection, id: i64, body: &str) -> Result<Note> {
    let body = normalize_note_body(body)?;
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
