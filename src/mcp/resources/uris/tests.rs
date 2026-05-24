use proptest::prelude::*;

use super::*;

fn provider_strategy() -> impl Strategy<Value = Provider> {
    prop::sample::select(Provider::all().to_vec())
}

fn source_name_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(LOCAL_SOURCE.to_string()),
        "[A-Za-z0-9][A-Za-z0-9_-]{0,24}"
            .prop_filter("source name must not be reserved", |name| name != "local"),
    ]
}

fn session_id_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z0-9][A-Za-z0-9_.:-]{0,80}"
}

proptest! {
    #[test]
    fn session_uris_roundtrip_generated_parts(
        source in source_name_strategy(),
        provider in provider_strategy(),
        session_id in session_id_strategy(),
    ) {
        let uri = session_uri_for_source(&source, provider, &session_id);

        let parsed = parse_aghist_uri(&uri).unwrap();

        match parsed {
            ParsedUri::Session {
                source: parsed_source,
                provider: parsed_provider,
                session_id: parsed_session_id,
            } => {
                let expected_source = (source != LOCAL_SOURCE).then_some(source);
                prop_assert_eq!(parsed_source, expected_source);
                prop_assert_eq!(parsed_provider, provider);
                prop_assert_eq!(parsed_session_id, session_id);
            }
            ParsedUri::Turn { .. } => prop_assert!(false, "session URI parsed as turn URI"),
        }
    }

    #[test]
    fn turn_uris_roundtrip_generated_parts(
        source in source_name_strategy(),
        provider in provider_strategy(),
        session_id in session_id_strategy(),
        turn in 1u32..=u32::MAX,
    ) {
        let uri = turn_uri_for_source(&source, provider, &session_id, turn);

        let parsed = parse_aghist_uri(&uri).unwrap();

        match parsed {
            ParsedUri::Turn {
                source: parsed_source,
                provider: parsed_provider,
                session_id: parsed_session_id,
                turn: parsed_turn,
            } => {
                let expected_source = (source != LOCAL_SOURCE).then_some(source);
                prop_assert_eq!(parsed_source, expected_source);
                prop_assert_eq!(parsed_provider, provider);
                prop_assert_eq!(parsed_session_id, session_id);
                prop_assert_eq!(parsed_turn, turn);
            }
            ParsedUri::Session { .. } => prop_assert!(false, "turn URI parsed as session URI"),
        }
    }
}

#[test]
fn rejects_control_characters_in_session_resource_id() {
    let Err(err) = parse_aghist_uri("aghist://session/codex-cli/abc\n123") else {
        panic!("session uri with control character should be rejected");
    };

    assert!(err.contains("control characters"));
}

#[test]
fn rejects_control_characters_in_turn_resource_id() {
    let Err(err) = parse_aghist_uri("aghist://session/codex-cli/abc\t123/turn/1") else {
        panic!("turn uri with control character should be rejected");
    };

    assert!(err.contains("control characters"));
}
