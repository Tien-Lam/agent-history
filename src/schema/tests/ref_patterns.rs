use crate::model::Provider;

use super::*;

#[test]
fn ref_patterns_track_provider_registry() {
    let providers = Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|");

    assert_eq!(
        common::source_qualified_session_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+(#[1-9][0-9]*)?$")
    );
    assert_eq!(
        common::source_qualified_session_only_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+$")
    );
    assert_eq!(
        common::source_qualified_citation_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+#[1-9][0-9]*$")
    );
    assert_eq!(
        common::todo_target_ref_pattern(),
        format!(
            "^(([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+|[a-z]{{2,}}-[a-z0-9.]*[0-9][a-z0-9.]*)$"
        )
    );
}

#[test]
fn provider_enum_tracks_provider_registry() {
    let expected: Vec<&str> = Provider::all().iter().map(|p| p.slug()).collect();
    let provider_enum = common::provider_slug_enum();
    let actual: Vec<&str> = provider_enum
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(actual, expected);
}
