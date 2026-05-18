use super::*;

#[test]
fn runtime_specs_track_provider_registry_order() {
    let runtime: Vec<Provider> = RUNTIME_PROVIDER_SPECS
        .iter()
        .map(|spec| spec.provider)
        .collect();
    assert_eq!(runtime, Provider::all());
}

#[test]
fn remote_candidate_dirs_cover_every_provider() {
    let root = Path::new("/tmp/aghist-root");
    for &provider in Provider::all() {
        assert_eq!(
            provider_from_dirs(provider, Vec::new()).provider(),
            provider
        );
        assert!(
            !remote_candidate_dirs(provider, root).is_empty(),
            "missing remote candidate dirs for {provider:?}"
        );
        assert_eq!(
            remote_candidate_dirs(provider, root).first(),
            Some(&root.to_path_buf()),
            "remote candidates should always include exact provider-dir roots first"
        );
    }
}

#[test]
fn runtime_specs_construct_matching_stateless_providers() {
    for spec in RUNTIME_PROVIDER_SPECS {
        assert_eq!(spec.stateless().provider(), spec.provider);
        assert_eq!(spec.from_dirs(Vec::new()).provider(), spec.provider);
    }
}
