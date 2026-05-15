mod common;

use common::provider_conformance::{
    assert_discover_load_roundtrip, assert_missing_dir_discovers_empty, generated_provider_cases,
    missing_dir_provider_cases,
};

#[test]
fn providers_with_missing_base_dirs_discover_empty() {
    let dir = tempfile::tempdir().unwrap();
    for case in missing_dir_provider_cases(dir.path()) {
        assert_missing_dir_discovers_empty(&case);
    }
}

#[test]
fn generated_providers_discover_and_load_messages() {
    let (_dirs, cases) = generated_provider_cases(2, 4);
    for case in &cases {
        assert_discover_load_roundtrip(case);
    }
}
