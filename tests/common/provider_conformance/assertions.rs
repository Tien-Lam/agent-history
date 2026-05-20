use std::collections::HashSet;

use aghist::model::{Message, Role, Session};
use aghist::provider::registry::provider_from_dirs;
use aghist::provider::{self};

use super::cases::ProviderCase;

pub fn assert_missing_dir_discovers_empty(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    assert_eq!(
        sessions.len(),
        case.expected_sessions.unwrap_or(0),
        "{} should discover no sessions for a missing root",
        case.label
    );
}

pub fn assert_discover_load_roundtrip(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    if let Some(expected) = case.expected_sessions {
        assert_eq!(
            sessions.len(),
            expected,
            "{} discovered an unexpected session count",
            case.label
        );
    }
    let mut seen_ids = HashSet::new();

    for session in &sessions {
        assert!(
            !session.id.0.is_empty(),
            "{} discovered a session with an empty id",
            case.label
        );
        assert!(
            seen_ids.insert(session.id.0.as_str()),
            "{} discovered duplicate session id {}",
            case.label,
            session.id.0
        );
        assert_eq!(
            session.provider, case.expected_provider,
            "{} discovered a session tagged with the wrong provider",
            case.label
        );
        let messages = case
            .provider
            .load_messages(session)
            .unwrap_or_else(|e| panic!("{} failed to load {}: {e}", case.label, session.id.0));
        if let Some(expected) = case.expected_messages_per_session {
            assert_eq!(
                messages.len(),
                expected,
                "{} loaded an unexpected message count for {}",
                case.label,
                session.id.0
            );
            assert_eq!(
                session.message_count, expected,
                "{} recorded an unexpected message_count for {}",
                case.label, session.id.0
            );
        } else {
            assert!(
                !messages.is_empty(),
                "{} should load messages for {}",
                case.label,
                session.id.0
            );
        }
        assert_eq!(
            session.message_count,
            messages.len(),
            "{} session {} message_count does not match loaded messages",
            case.label,
            session.id.0
        );
        assert_messages_are_well_formed(case, session, &messages);
    }
}

pub fn assert_registry_constructor_roundtrip(case: &ProviderCase) {
    let reconstructed =
        provider_from_dirs(case.expected_provider, case.provider.base_dirs().to_vec());
    let reconstructed = ProviderCase {
        label: case.label,
        provider: reconstructed,
        expected_provider: case.expected_provider,
        expected_sessions: case.expected_sessions,
        expected_messages_per_session: case.expected_messages_per_session,
    };
    assert_discover_load_roundtrip(&reconstructed);
}

pub fn assert_stateless_loader_roundtrip(case: &ProviderCase) {
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    for session in &sessions {
        let direct = case
            .provider
            .load_messages(session)
            .unwrap_or_else(|e| panic!("{} failed to load {}: {e}", case.label, session.id.0));
        let stateless = provider::load_messages_for_session(session, &[]).unwrap_or_else(|e| {
            panic!(
                "{} stateless load failed for {}: {e}",
                case.label, session.id.0
            )
        });
        assert_eq!(
            serde_json::to_value(&stateless).unwrap(),
            serde_json::to_value(&direct).unwrap(),
            "{} stateless loader drifted for {}",
            case.label,
            session.id.0
        );
    }
}

fn assert_messages_are_well_formed(case: &ProviderCase, session: &Session, messages: &[Message]) {
    for (idx, message) in messages.iter().enumerate() {
        assert!(
            !message.id.0.is_empty(),
            "{} loaded message {} in {} with an empty id",
            case.label,
            idx,
            session.id.0
        );
        assert!(
            !message.content.is_empty(),
            "{} loaded message {} in {} with no content blocks",
            case.label,
            message.id.0,
            session.id.0
        );
        if case.expected_messages_per_session.is_some() {
            let expected = generated_fixture_role(idx);
            assert_eq!(
                message.role,
                expected,
                "{} loaded message {} in {} with role {}, expected {}",
                case.label,
                message.id.0,
                session.id.0,
                message.role.slug(),
                expected.slug()
            );
        }
    }
}

fn generated_fixture_role(idx: usize) -> Role {
    if idx.is_multiple_of(2) {
        Role::User
    } else {
        Role::Assistant
    }
}
