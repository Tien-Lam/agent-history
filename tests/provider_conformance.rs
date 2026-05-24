mod common;

use aghist::model::Provider;
use common::provider_conformance::assertions::{
    assert_discover_load_roundtrip, assert_missing_dir_discovers_empty,
    assert_registry_constructor_roundtrip, assert_stateless_loader_roundtrip,
};
use common::provider_conformance::cases::{
    generated_provider_cases, missing_dir_provider_cases, static_fixture_provider_cases,
};
use common::provider_conformance::contract::provider_contract_json;
use std::panic::AssertUnwindSafe;

#[test]
fn missing_dir_cases_cover_every_provider() {
    let dir = tempfile::tempdir().unwrap();
    let providers: Vec<Provider> = missing_dir_provider_cases(dir.path())
        .iter()
        .map(|case| case.expected_provider)
        .collect();
    assert_eq!(providers, Provider::all());
}

#[test]
fn providers_with_missing_base_dirs_discover_empty() {
    let dir = tempfile::tempdir().unwrap();
    for case in missing_dir_provider_cases(dir.path()) {
        assert_missing_dir_discovers_empty(&case);
    }
}

#[test]
fn generated_cases_cover_every_provider() {
    let cases = generated_provider_cases(1, 1);
    assert_eq!(cases.providers(), Provider::all());
}

#[test]
fn generated_providers_discover_and_load_messages() {
    let cases = generated_provider_cases(2, 4);
    for case in cases.cases() {
        assert_discover_load_roundtrip(case);
    }
}

#[test]
fn static_fixture_providers_discover_and_load_messages() {
    for case in static_fixture_provider_cases() {
        assert_discover_load_roundtrip(&case);
    }
}

#[test]
fn generated_provider_loaders_do_not_panic_on_malformed_source_paths() {
    let cases = generated_provider_cases(1, 2);
    let malformed_root = tempfile::tempdir().unwrap();

    for case in cases.cases() {
        let sessions = case
            .provider
            .discover_sessions()
            .unwrap_or_else(|err| panic!("{} discovery failed: {err}", case.label));
        let Some(session) = sessions.first() else {
            panic!("{} generated fixture should discover a session", case.label);
        };

        let mut malformed_session = session.clone();
        malformed_session.source_path =
            malformed_source_path(malformed_root.path(), &malformed_session);

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            case.provider.load_messages_with_stats(&malformed_session)
        }));

        assert!(
            result.is_ok(),
            "{} loader panicked on malformed source path {}",
            case.label,
            malformed_session.source_path.display()
        );
    }
}

#[test]
fn registry_constructors_preserve_generated_provider_behavior() {
    let cases = generated_provider_cases(2, 4);
    for case in cases.cases() {
        assert_registry_constructor_roundtrip(case);
    }
}

#[test]
fn stateless_loader_can_read_generated_sessions() {
    let cases = generated_provider_cases(1, 4);
    for case in cases.cases() {
        assert_stateless_loader_roundtrip(case);
    }
}

#[test]
fn generated_provider_contract_snapshot() {
    let cases = generated_provider_cases(1, 3);
    insta::assert_snapshot!(
        "generated_provider_contract",
        provider_contract_json(cases.cases())
    );
}

fn malformed_source_path(
    root: &std::path::Path,
    session: &aghist::model::Session,
) -> std::path::PathBuf {
    let path = root.join(session.provider.slug());
    match session.provider {
        Provider::CopilotCli => {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("events.jsonl"), "not-json\n{\"type\":3}\n").unwrap();
        }
        Provider::OpenCode => {
            let message_dir = path.join("message").join(&session.id.0);
            std::fs::create_dir_all(&message_dir).unwrap();
            std::fs::write(message_dir.join("bad.json"), b"{not-json").unwrap();
            std::fs::create_dir_all(path.join("part")).unwrap();
        }
        Provider::Cline => {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("api_conversation_history.json"), b"{not-json").unwrap();
        }
        _ => {
            std::fs::write(&path, b"{not-json\n").unwrap();
        }
    }
    path
}
