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
