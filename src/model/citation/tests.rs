use super::*;

fn sid(s: &str) -> SessionId {
    SessionId(s.to_string())
}

#[test]
fn display_uses_provider_slug() {
    let r = CitationRef {
        provider: Provider::ClaudeCode,
        session_id: sid("abc-123"),
        turn: 7,
    };
    assert_eq!(r.to_string(), "claude-code/abc-123#7");
}

#[test]
fn parse_basic() {
    let r: CitationRef = "claude-code/abc-123#7".parse().unwrap();
    assert_eq!(r.provider, Provider::ClaudeCode);
    assert_eq!(r.session_id, sid("abc-123"));
    assert_eq!(r.turn, 7);
}

#[test]
fn session_ref_round_trips() {
    let r: SessionRef = "claude-code/abc-123".parse().unwrap();
    assert_eq!(r.provider, Provider::ClaudeCode);
    assert_eq!(r.session_id, sid("abc-123"));
    assert_eq!(r.to_string(), "claude-code/abc-123");
    assert_eq!(r.turn(7).unwrap().to_string(), "claude-code/abc-123#7");
}

#[test]
fn session_or_turn_ref_round_trips_both_shapes() {
    let session: SessionOrTurnRef = "claude-code/abc-123".parse().unwrap();
    assert_eq!(session.to_string(), "claude-code/abc-123");
    assert_eq!(session.turn(), None);

    let turn: SessionOrTurnRef = "claude-code/abc-123#7".parse().unwrap();
    assert_eq!(turn.to_string(), "claude-code/abc-123#7");
    assert_eq!(turn.session_ref().to_string(), "claude-code/abc-123");
    assert_eq!(turn.turn(), Some(7));
}

#[test]
fn qualified_citation_ref_round_trips_source_prefix() {
    let local: QualifiedCitationRef = "claude-code/abc-123#7".parse().unwrap();
    assert_eq!(local.source, None);
    assert_eq!(local.to_string(), "claude-code/abc-123#7");

    let remote: QualifiedCitationRef = "workbox:claude-code/abc-123#7".parse().unwrap();
    assert_eq!(remote.source.as_deref(), Some("workbox"));
    assert_eq!(remote.to_string(), "workbox:claude-code/abc-123#7");
}

#[test]
fn round_trip_all_providers() {
    for &p in Provider::all() {
        let original = CitationRef {
            provider: p,
            session_id: sid("ses-uuid-0001"),
            turn: 42,
        };
        let rendered = original.to_string();
        let parsed: CitationRef = rendered.parse().unwrap();
        assert_eq!(parsed, original, "round-trip for {p:?} ({rendered})");
    }
}

#[test]
fn round_trip_complex_session_ids() {
    let ids = [
        "rollout-2024-03-15T10-30-00-a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "ses_abc123",
        "uuid-789-with-many-segments-and-longer-tail-0001",
    ];
    for id in ids {
        let original = CitationRef {
            provider: Provider::CodexCli,
            session_id: sid(id),
            turn: 1,
        };
        let parsed: CitationRef = original.to_string().parse().unwrap();
        assert_eq!(parsed, original);
    }
}

#[test]
fn parse_rejects_empty() {
    assert_eq!("".parse::<CitationRef>(), Err(CitationParseError::Empty));
}

#[test]
fn parse_rejects_missing_turn() {
    assert_eq!(
        "claude-code/abc".parse::<CitationRef>(),
        Err(CitationParseError::MissingTurn)
    );
    assert_eq!(
        "claude-code/abc#".parse::<CitationRef>(),
        Err(CitationParseError::MissingTurn)
    );
}

#[test]
fn parse_rejects_missing_session_id() {
    assert_eq!(
        "claude-code#5".parse::<CitationRef>(),
        Err(CitationParseError::MissingSessionId)
    );
    assert_eq!(
        "claude-code/#5".parse::<CitationRef>(),
        Err(CitationParseError::MissingSessionId)
    );
}

#[test]
fn parse_rejects_missing_provider() {
    assert_eq!(
        "/abc#5".parse::<CitationRef>(),
        Err(CitationParseError::MissingProvider)
    );
}

#[test]
fn parse_rejects_unknown_provider() {
    assert_eq!(
        "Claude-Code/abc#5".parse::<CitationRef>(),
        Err(CitationParseError::UnknownProvider("Claude-Code".into()))
    );
    assert_eq!(
        "fake-provider/abc#5".parse::<CitationRef>(),
        Err(CitationParseError::UnknownProvider("fake-provider".into()))
    );
}

#[test]
fn parse_rejects_zero_turn() {
    assert_eq!(
        "claude-code/abc#0".parse::<CitationRef>(),
        Err(CitationParseError::InvalidTurn("0".into()))
    );
}

#[test]
fn parse_rejects_non_numeric_turn() {
    assert_eq!(
        "claude-code/abc#seven".parse::<CitationRef>(),
        Err(CitationParseError::InvalidTurn("seven".into()))
    );
    assert_eq!(
        "claude-code/abc#-1".parse::<CitationRef>(),
        Err(CitationParseError::InvalidTurn("-1".into()))
    );
}

#[test]
fn new_validates_inputs() {
    assert!(CitationRef::new(Provider::ClaudeCode, sid("abc"), 1).is_some());
    assert!(CitationRef::new(Provider::ClaudeCode, sid("abc"), 0).is_none());
    assert!(CitationRef::new(Provider::ClaudeCode, sid(""), 1).is_none());
}
