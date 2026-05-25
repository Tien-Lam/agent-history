use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::{refs::SessionRefPredicate, validate_session_ref, MetadataError, Result};

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
            let predicate = SessionRefPredicate::parse(raw)?;
            let sql = format!(
                "SELECT {columns} FROM stars WHERE {} {order}",
                predicate.clause()
            );
            let mut stmt = conn.prepare(&sql)?;
            let params_vec = predicate.params();
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params_vec.iter().map(AsRef::as_ref).collect();
            let rows = stmt.query_map(param_refs.as_slice(), row_to_star)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
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
    conn.execute(
        "DELETE FROM stars WHERE session_ref = ?1",
        params![session_ref],
    )?;
    Ok(existing)
}
