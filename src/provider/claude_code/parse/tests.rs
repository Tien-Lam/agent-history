use super::*;

#[test]
fn decode_project_name_basic() {
    assert_eq!(
        decode_project_name("V--Projects-agent-history"),
        "V:/Projects-agent-history"
    );
}
