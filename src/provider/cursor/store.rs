use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::format::{millis_value_to_datetime, ComposerData, HeaderEntry};
use super::message::{build_message_result, BuildMessageResult};
use super::ProviderError;
use crate::model::{Message, Provider, Session, SessionId};
use crate::provider::json_text::{stringish, value_u8};
use crate::provider::project_name_from_path;
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

pub(crate) fn state_db_path(base: &Path) -> PathBuf {
    base.join("User").join("globalStorage").join("state.vscdb")
}

pub(crate) fn read_sessions(db_path: &Path) -> Result<Vec<Session>, ProviderError> {
    let conn = open_readonly(db_path)?;

    // The cursorDiskKV table may not exist on a fresh install — treat
    // missing-table as zero sessions rather than an error.
    if !table_exists(&conn, "cursorDiskKV")? {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%'")
        .map_err(sql_err(db_path))?;

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        })
        .map_err(sql_err(db_path))?;

    let mut sessions = Vec::new();
    let mut row_count: usize = 0;
    let mut parse_failures: usize = 0;
    for row in rows {
        let (key, value) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "skipping malformed Cursor row");
                continue;
            }
        };
        row_count += 1;
        match build_session_from_row(&key, &value, db_path) {
            Some(s) => sessions.push(s),
            None => parse_failures += 1,
        }
    }
    tracing::info!(
        path = %db_path.display(),
        rows = row_count,
        parse_failures,
        sessions = sessions.len(),
        "Cursor session discovery complete"
    );
    Ok(sessions)
}

fn build_session_from_row(key: &str, value: &[u8], db_path: &Path) -> Option<Session> {
    let composer_id = key.strip_prefix("composerData:")?.to_string();

    let raw: ComposerData = match serde_json::from_slice(value) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "failed to parse composer JSON");
            return None;
        }
    };

    let id = stringish(raw.composer_id.as_ref(), &["composerId", "id"]).unwrap_or(composer_id);

    let started_at = raw
        .created_at
        .as_ref()
        .and_then(millis_value_to_datetime)
        .or_else(|| {
            raw.last_updated_at
                .as_ref()
                .and_then(millis_value_to_datetime)
        })?;
    let ended_at = raw
        .last_updated_at
        .as_ref()
        .and_then(millis_value_to_datetime);

    let workspace_folder = stringish(
        raw.workspace_folder.as_ref(),
        &["currentWorkspaceFolder", "workspace", "path"],
    );
    let project_path = workspace_folder.clone().map(PathBuf::from);
    let project_name = workspace_folder.as_deref().and_then(project_name_from_path);

    let message_count = raw
        .headers
        .iter()
        .filter(|e| stringish(e.bubble_id.as_ref(), &["bubbleId", "id"]).is_some())
        .count();

    Some(Session {
        id: SessionId(id),
        provider: Provider::Cursor,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: stringish(raw.name.as_ref(), &["name", "title", "summary"]),
        model: stringish(raw.model.as_ref(), &["model", "id", "name"]),
        token_usage: None,
        message_count,
        source_path: db_path.to_path_buf(),
    })
}

pub(crate) fn load_messages_from_db_with_stats(
    db_path: &Path,
    composer_id: &str,
) -> Result<ProviderMessageLoad, ProviderError> {
    if !db_path.exists() {
        return Ok(ProviderMessageLoad::from_messages(Vec::new()));
    }
    let conn = open_readonly(db_path)?;
    if !table_exists(&conn, "cursorDiskKV")? {
        return Ok(ProviderMessageLoad::from_messages(Vec::new()));
    }

    // 1. Read composer header to recover bubble order.
    let composer_key = format!("composerData:{composer_id}");
    let composer: Option<ComposerData> = conn
        .query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1",
            [&composer_key],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let headers: Vec<HeaderEntry> = composer
        .map(|c| c.headers)
        .unwrap_or_default()
        .into_iter()
        .filter(|h| stringish(h.bubble_id.as_ref(), &["bubbleId", "id"]).is_some())
        .collect();

    // 2. Read each bubble keyed under this composer. We collect both ways:
    //    headers give canonical ordering; a fallback LIKE scan catches bubbles
    //    not listed in the header (shouldn't happen in practice but matches
    //    the "tolerate corrupt" stance).
    let mut messages = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut parse_stats = ProviderParseStats::default();

    for (idx, h) in headers.iter().enumerate() {
        let Some(bid) = stringish(h.bubble_id.as_ref(), &["bubbleId", "id"]) else {
            continue;
        };
        let key = format!("bubbleId:{composer_id}:{bid}");
        if let Some(bytes) = read_value(&conn, &key)? {
            parse_stats.record_seen();
            seen.insert(bid.clone());
            let header_type = value_u8(h.bubble_type.as_ref(), &["type", "value"]);
            push_cursor_message_result(
                build_message_result(&bid, header_type, &bytes, idx),
                &mut messages,
                &mut parse_stats,
            );
        }
    }

    // Fallback scan: pick up any orphan bubbles. Sort by createdAt to
    // approximate the original ordering.
    let pattern = format!("bubbleId:{composer_id}:%");
    let mut stmt = conn
        .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE ?1")
        .map_err(sql_err(db_path))?;
    let rows = stmt
        .query_map([&pattern], |row| {
            let key: String = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        })
        .map_err(sql_err(db_path))?;
    let mut orphans: Vec<Message> = Vec::new();
    for row in rows.flatten() {
        let (key, value) = row;
        let prefix = format!("bubbleId:{composer_id}:");
        let Some(bid) = key.strip_prefix(&prefix) else {
            continue;
        };
        if seen.contains(bid) {
            continue;
        }
        parse_stats.record_seen();
        push_cursor_message_result(
            build_message_result(bid, None, &value, messages.len() + orphans.len()),
            &mut orphans,
            &mut parse_stats,
        );
    }
    orphans.sort_by_key(|m| m.timestamp);
    messages.extend(orphans);

    Ok(ProviderMessageLoad {
        messages,
        parse_stats,
    })
}

fn push_cursor_message_result(
    result: BuildMessageResult,
    messages: &mut Vec<Message>,
    parse_stats: &mut ProviderParseStats,
) {
    match result {
        BuildMessageResult::Message(message) => {
            if message.content.is_empty() {
                parse_stats.record_empty_content();
            }
            messages.push(message);
        }
        BuildMessageResult::ParseError => parse_stats.record_parse_error(),
        BuildMessageResult::SkippedRecord => parse_stats.record_skipped_record(),
    }
}

fn read_value(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, ProviderError> {
    match conn.query_row(
        "SELECT value FROM cursorDiskKV WHERE key = ?1",
        [key],
        |row| row.get::<_, Vec<u8>>(0),
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(ProviderError::Parse {
            path: PathBuf::from(key),
            reason: e.to_string(),
        }),
    }
}

fn open_readonly(db_path: &Path) -> Result<Connection, ProviderError> {
    Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(sql_err(db_path))
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, ProviderError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .map(|()| true)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(ProviderError::Parse {
            path: PathBuf::new(),
            reason: other.to_string(),
        }),
    })
}

fn sql_err(path: &Path) -> impl Fn(rusqlite::Error) -> ProviderError + '_ {
    move |e: rusqlite::Error| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}
