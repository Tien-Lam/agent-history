use super::*;

#[test]
fn validate_accepts_session_and_turn_refs() {
    validate_session_ref("claude-code/abc-123").unwrap();
    validate_session_ref("claude-code/abc-123#7").unwrap();
    validate_session_ref("claude-code/abc:def#7").unwrap();
    validate_session_ref("opencode/ses_xyz#99").unwrap();
    validate_session_ref("laptop:claude-code/abc-123").unwrap();
    validate_session_ref("work_box:opencode/ses_xyz#99").unwrap();
}

#[test]
fn validate_rejects_bad_refs() {
    assert!(matches!(
        validate_session_ref(""),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("no-slash"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("Claude-Code/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/abc#0"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/abc#two"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("-bad:claude-code/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("local:claude-code/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}

#[test]
fn session_key_from_ref_validates_and_strips_turn_suffix() {
    assert_eq!(
        session_key_from_ref("claude-code/abc-123#7").unwrap(),
        "claude-code/abc-123"
    );
    assert_eq!(
        session_key_from_ref("laptop:claude-code/abc-123#7").unwrap(),
        "laptop:claude-code/abc-123"
    );
    assert!(matches!(
        session_key_from_ref("local:claude-code/abc-123#7"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}
