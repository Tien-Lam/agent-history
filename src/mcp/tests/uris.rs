use super::*;

#[test]
fn parse_uri_session_form() {
    let p = parse_aghist_uri("aghist://session/claude-code/abc-123").unwrap();
    match p {
        ParsedUri::Session {
            source,
            provider,
            session_id,
        } => {
            assert_eq!(source, None);
            assert_eq!(provider, Provider::ClaudeCode);
            assert_eq!(session_id, "abc-123");
        }
        ParsedUri::Turn { .. } => panic!("expected Session form"),
    }
}

#[test]
fn parse_uri_turn_form() {
    let p = parse_aghist_uri("aghist://session/codex-cli/ses_abc/turn/7").unwrap();
    match p {
        ParsedUri::Turn {
            source,
            provider,
            session_id,
            turn,
        } => {
            assert_eq!(source, None);
            assert_eq!(provider, Provider::CodexCli);
            assert_eq!(session_id, "ses_abc");
            assert_eq!(turn, 7);
        }
        ParsedUri::Session { .. } => panic!("expected Turn form"),
    }
}

#[test]
fn parse_uri_session_id_with_slash_in_path_is_treated_as_session_id() {
    // No real provider emits these today, but if a session id ever contains
    // a `/`, anything before `/turn/<n>` should still parse as the id.
    let p = parse_aghist_uri("aghist://session/claude-code/foo/bar/turn/3").unwrap();
    match p {
        ParsedUri::Turn {
            session_id, turn, ..
        } => {
            assert_eq!(session_id, "foo/bar");
            assert_eq!(turn, 3);
        }
        ParsedUri::Session { .. } => panic!("expected Turn form"),
    }
}

#[test]
fn parse_uri_rejects_bad_inputs() {
    assert!(parse_aghist_uri("file:///etc/passwd").is_err());
    assert!(parse_aghist_uri("aghist://session/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/abc").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/0").is_err());
}

#[test]
fn session_uri_round_trips_through_parser() {
    let uri = session_uri(Provider::OpenCode, "session-xyz");
    assert_eq!(uri, "aghist://session/opencode/session-xyz");
    let parsed = parse_aghist_uri(&uri).unwrap();
    match parsed {
        ParsedUri::Session {
            source,
            provider,
            session_id,
        } => {
            assert_eq!(source, None);
            assert_eq!(provider, Provider::OpenCode);
            assert_eq!(session_id, "session-xyz");
        }
        ParsedUri::Turn { .. } => panic!("expected Session"),
    }
}

#[test]
fn turn_uri_round_trips_through_parser() {
    let uri = turn_uri(Provider::GeminiCli, "g-1", 42);
    assert_eq!(uri, "aghist://session/gemini-cli/g-1/turn/42");
    let parsed = parse_aghist_uri(&uri).unwrap();
    match parsed {
        ParsedUri::Turn {
            source,
            provider,
            session_id,
            turn,
        } => {
            assert_eq!(source, None);
            assert_eq!(provider, Provider::GeminiCli);
            assert_eq!(session_id, "g-1");
            assert_eq!(turn, 42);
        }
        ParsedUri::Session { .. } => panic!("expected Turn"),
    }
}

#[test]
fn source_qualified_uris_round_trip_through_parser() {
    let session = session_uri_for_source("laptop", Provider::ClaudeCode, "abc-123");
    assert_eq!(
        session,
        "aghist://source/laptop/session/claude-code/abc-123"
    );
    match parse_aghist_uri(&session).unwrap() {
        ParsedUri::Session {
            source,
            provider,
            session_id,
        } => {
            assert_eq!(source.as_deref(), Some("laptop"));
            assert_eq!(provider, Provider::ClaudeCode);
            assert_eq!(session_id, "abc-123");
        }
        ParsedUri::Turn { .. } => panic!("expected Session"),
    }

    let turn = turn_uri_for_source("laptop", Provider::ClaudeCode, "abc-123", 9);
    assert_eq!(
        turn,
        "aghist://source/laptop/session/claude-code/abc-123/turn/9"
    );
    match parse_aghist_uri(&turn).unwrap() {
        ParsedUri::Turn {
            source,
            provider,
            session_id,
            turn,
        } => {
            assert_eq!(source.as_deref(), Some("laptop"));
            assert_eq!(provider, Provider::ClaudeCode);
            assert_eq!(session_id, "abc-123");
            assert_eq!(turn, 9);
        }
        ParsedUri::Session { .. } => panic!("expected Turn"),
    }
}
