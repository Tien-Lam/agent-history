use std::collections::HashSet;

use rusqlite::{params, Connection};

use super::tags::normalize_tag;
use super::{MetadataError, Result};

/// Resolve metadata-filter flags into a set of `<provider-slug>/<session-id>`
/// keys (turn suffix stripped). Used by `--list` / `search` to keep only
/// sessions that match the requested annotations.
///
/// All active filters AND-combine: a session is kept only if it appears in
/// every requested filter's key set. `note_substr` is a case-insensitive
/// substring match against the note body; `tag` is an exact tag value;
/// `starred` requires at least one star (session-level OR any of its turns).
///
/// Returns `Ok(None)` when no metadata filter is active. Returns
/// `Ok(Some(set))` otherwise — possibly an empty set if filters match nothing.
pub fn filter_session_keys(
    conn: &Connection,
    note_substr: Option<&str>,
    tag: Option<&str>,
    starred: bool,
) -> Result<Option<HashSet<String>>> {
    let mut sets: Vec<HashSet<String>> = Vec::new();

    if let Some(needle) = note_substr {
        let needle = needle.trim();
        if needle.is_empty() {
            return Err(MetadataError::EmptyBody);
        }
        let pattern = format!("%{needle}%");
        let mut stmt = conn
            .prepare("SELECT DISTINCT session_ref FROM notes WHERE body LIKE ?1 COLLATE NOCASE")?;
        let refs = stmt
            .query_map(params![pattern], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        sets.push(refs.into_iter().map(strip_turn_suffix).collect());
    }

    if let Some(tag) = tag {
        let normalized = normalize_tag(tag)?;
        let mut stmt = conn.prepare("SELECT DISTINCT session_ref FROM tags WHERE tag = ?1")?;
        let refs = stmt
            .query_map(params![normalized], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        sets.push(refs.into_iter().map(strip_turn_suffix).collect());
    }

    if starred {
        let mut stmt = conn.prepare("SELECT DISTINCT session_ref FROM stars")?;
        let refs = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        sets.push(refs.into_iter().map(strip_turn_suffix).collect());
    }

    if sets.is_empty() {
        return Ok(None);
    }

    let mut iter = sets.into_iter();
    let mut acc = iter.next().expect("non-empty");
    for next in iter {
        acc.retain(|k| next.contains(k));
    }
    Ok(Some(acc))
}

/// Strip the `#<turn>` suffix from a `session_ref`, leaving the
/// `<provider-slug>/<session-id>` prefix.
fn strip_turn_suffix(session_ref: String) -> String {
    match session_ref.rsplit_once('#') {
        Some((prefix, _)) => prefix.to_string(),
        None => session_ref,
    }
}
