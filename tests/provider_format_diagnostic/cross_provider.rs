use super::common::provider_conformance::assertions::assert_discover_load_roundtrip;
use super::common::provider_conformance::cases::static_fixture_provider_cases;

#[test]
fn all_fixture_providers_roundtrip() {
    for case in static_fixture_provider_cases() {
        assert_discover_load_roundtrip(&case);
    }
}
