use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::super::format::{ComposerData, HeaderEntry};
use super::super::message::{build_message_result, BuildMessageResult};
use super::super::ProviderError;
use crate::model::Message;
use crate::provider::json_text::{stringish, value_u8};
use crate::provider::{ProviderMessageLoad, ProviderParseStats};

pub(crate) fn load_messages_from_db_with_stats(
    db_path: &Path,
    composer_id: &str,
) -> Result<ProviderMessageLoad, ProviderError> {
    if !db_path.exists() {
        return Ok(ProviderMessageLoad::from_messages(Vec::new()));
    }
    let conn = super::open_readonly(db_path)?;
    if !super::table_exists(&conn, "cursorDiskKV")? {
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
        .map_err(super::sql_err(db_path))?;
    let rows = stmt
        .query_map([&pattern], |row| {
            let key: String = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        })
        .map_err(super::sql_err(db_path))?;
    let mut orphans: Vec<Message> = Vec::new();
    for row in rows {
        let (key, value) = row.map_err(super::sql_err(db_path))?;
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
