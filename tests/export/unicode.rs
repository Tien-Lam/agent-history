use std::path::PathBuf;

use aghist::export;

#[test]
fn export_handles_unicode_content() {
    use aghist::model::*;
    use chrono::Utc;

    let session = Session {
        id: SessionId("unicode-test".into()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("プロジェクト".into()),
        git_branch: Some("feature/日本語".into()),
        started_at: Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from("/tmp/test"),
    };
    let messages = vec![Message {
        id: MessageId("m1".into()),
        role: Role::User,
        timestamp: Utc::now(),
        content: vec![
            ContentBlock::Text("你好世界 🌍 مرحبا".into()),
            ContentBlock::CodeBlock {
                language: Some("python".into()),
                code: "print('café ☕')".into(),
            },
        ],
        model: None,
        token_usage: None,
    }];

    let md = export::to_markdown(&session, &messages);
    assert!(
        md.contains("プロジェクト"),
        "markdown should preserve CJK project name"
    );
    assert!(
        md.contains("你好世界"),
        "markdown should preserve CJK content"
    );
    assert!(md.contains("🌍"), "markdown should preserve emoji");

    let json = export::to_json(&session, &messages);
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("JSON should be valid with unicode");
    assert_eq!(parsed["session"]["project_name"], "プロジェクト");

    let html = export::to_html(&session, &messages);
    assert!(html.contains("プロジェクト"), "HTML should preserve CJK");
    assert!(html.contains("مرحبا"), "HTML should preserve RTL text");
}
