use super::super::aghist;

#[test]
fn schema_search_includes_metadata_filter_params() {
    let out = aghist().args(["schema", "search"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert!(
        props["note"].is_object(),
        "search schema missing 'note' param"
    );
    assert!(
        props["tag"].is_object(),
        "search schema missing 'tag' param"
    );
    assert!(
        props["starred"].is_object(),
        "search schema missing 'starred' param"
    );
    assert_eq!(
        props["note"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_NOTE_FILTER_MAX_BYTES)
    );
    assert_eq!(
        props["tag"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_TAG_MAX_BYTES)
    );
    assert_eq!(props["starred"]["type"], "boolean");
}
