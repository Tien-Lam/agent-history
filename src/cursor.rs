//! Opaque base64 cursors for keyset pagination of `--list` and `search`.
//!
//! Cursors are deliberately *not* offsets: callers that resume on a shifting
//! result set (newly indexed sessions, re-ranked search hits) must not
//! silently skip or duplicate items. We encode the last item's sort key
//! instead, so the next page can be derived by "items strictly after this
//! key in the canonical sort order".
//!
//! `SearchCursor` carries the full search sort key. `ListCursor` carries the
//! session's `started_at` (primary sort, descending) plus the session identity
//! key so local and remote sessions with reused ids paginate cleanly.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CursorError {
    #[error("invalid cursor: not valid base64")]
    Base64,
    #[error("invalid cursor: malformed payload")]
    Payload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchCursor {
    pub score: f32,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub message_key: String,
    #[serde(default)]
    pub message_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub note_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListCursor {
    pub started_at: DateTime<Utc>,
    pub session_id: String,
    #[serde(default)]
    pub session_key: String,
}

impl SearchCursor {
    pub fn encode(&self) -> String {
        encode(self)
    }

    pub fn decode(token: &str) -> Result<Self, CursorError> {
        decode(token)
    }
}

impl ListCursor {
    pub fn encode(&self) -> String {
        encode(self)
    }

    pub fn decode(token: &str) -> Result<Self, CursorError> {
        decode(token)
    }
}

fn encode<T: Serialize>(value: &T) -> String {
    // serde_json on a small struct cannot fail, but if it ever does we'd
    // rather emit an empty cursor than poison the response.
    let json = serde_json::to_vec(value).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(json)
}

fn decode<T: for<'de> Deserialize<'de>>(token: &str) -> Result<T, CursorError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(token.trim())
        .map_err(|_| CursorError::Base64)?;
    serde_json::from_slice(&bytes).map_err(|_| CursorError::Payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn utc_datetime_strategy() -> impl Strategy<Value = DateTime<Utc>> {
        (0i64..4_102_444_800, 0u32..1_000_000_000)
            .prop_map(|(secs, nanos)| Utc.timestamp_opt(secs, nanos).single().unwrap())
    }

    fn cursor_string_strategy() -> impl Strategy<Value = String> {
        "[A-Za-z0-9_./:\\-]{0,80}"
    }

    #[test]
    fn search_cursor_roundtrip() {
        let c = SearchCursor {
            score: 1.5,
            started_at: Some(Utc.with_ymd_and_hms(2026, 5, 7, 1, 14, 0).unwrap()),
            session_key: "claude-code\x1fabc-123\x1f/tmp/session.jsonl".to_string(),
            session_id: "abc-123".to_string(),
            message_key: "claude-code\x1fabc-123\x1f/tmp/session.jsonl\x1f0\x1fmsg-1".to_string(),
            message_id: "msg-1".to_string(),
            kind: "message".to_string(),
            note_id: None,
        };
        let token = c.encode();
        let back = SearchCursor::decode(&token).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn list_cursor_roundtrip() {
        let c = ListCursor {
            started_at: Utc.with_ymd_and_hms(2026, 5, 7, 1, 14, 0).unwrap(),
            session_id: "xyz".to_string(),
            session_key: "claude-code\x1fxyz\x1f/tmp/session.jsonl".to_string(),
        };
        let token = c.encode();
        let back = ListCursor::decode(&token).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn cursor_token_is_url_safe() {
        let c = SearchCursor {
            score: 0.0,
            started_at: None,
            session_key: String::new(),
            session_id: "id-with/slash+plus".to_string(),
            message_key: String::new(),
            message_id: String::new(),
            kind: "message".to_string(),
            note_id: None,
        };
        let token = c.encode();
        assert!(!token.contains('+'));
        assert!(!token.contains('/'));
        assert!(!token.contains('='));
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert!(matches!(
            SearchCursor::decode("not-base64!!!"),
            Err(CursorError::Base64)
        ));
        assert!(matches!(
            SearchCursor::decode("aGVsbG8"), // valid b64 of "hello", invalid JSON
            Err(CursorError::Payload)
        ));
    }

    proptest! {
        #[test]
        fn list_cursor_roundtrips_generated_values(
            started_at in utc_datetime_strategy(),
            session_id in cursor_string_strategy(),
            session_key in cursor_string_strategy(),
        ) {
            prop_assume!(!session_id.is_empty());
            let cursor = ListCursor {
                started_at,
                session_id,
                session_key,
            };

            let decoded = ListCursor::decode(&cursor.encode()).unwrap();

            prop_assert_eq!(decoded, cursor);
        }

        #[test]
        fn search_cursor_roundtrips_generated_values(
            score in -1_000_000.0f32..1_000_000.0,
            started_at in prop::option::of(utc_datetime_strategy()),
            session_key in cursor_string_strategy(),
            session_id in cursor_string_strategy(),
            message_key in cursor_string_strategy(),
            message_id in cursor_string_strategy(),
            kind in prop::sample::select(vec!["message".to_string(), "note".to_string()]),
            note_id in prop::option::of(0i64..1_000_000),
        ) {
            let cursor = SearchCursor {
                score,
                started_at,
                session_key,
                session_id,
                message_key,
                message_id,
                kind,
                note_id,
            };

            let decoded = SearchCursor::decode(&cursor.encode()).unwrap();

            prop_assert_eq!(decoded, cursor);
        }
    }
}
