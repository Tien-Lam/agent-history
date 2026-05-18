mod common;

use aghist::model::Provider;
use common::provider_conformance::{
    assert_discover_load_roundtrip, assert_missing_dir_discovers_empty,
    assert_registry_constructor_roundtrip, assert_stateless_loader_roundtrip,
    generated_provider_cases, missing_dir_provider_cases, provider_contract_json,
};

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
