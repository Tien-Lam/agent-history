use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::schema_fragments::METADATA_TAG_MAX_BYTES;

use super::{refs::turn_prefix_like_pattern, validate_session_ref, MetadataError, Result};

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
/// requires that they be non-empty after trimming and fit within the public
/// schema limit. Uniqueness per `session_ref` is enforced by the schema.
pub(super) fn normalize_tag(tag: &str) -> Result<&str> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err(MetadataError::EmptyTag);
    }
    if trimmed.len() > METADATA_TAG_MAX_BYTES {
        return Err(MetadataError::TagTooLong {
            bytes: trimmed.len(),
            max_bytes: METADATA_TAG_MAX_BYTES,
        });
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
    let tag = conn.query_row(&sql, params![id], row_to_tag).optional()?;
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
            clauses.push("(session_ref = ? OR session_ref LIKE ? ESCAPE '\\')");
            params_vec.push(Box::new(raw.to_string()));
            params_vec.push(Box::new(turn_prefix_like_pattern(raw)));
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
        format!(
            "SELECT {columns} FROM tags WHERE {} {order}",
            clauses.join(" AND ")
        )
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
