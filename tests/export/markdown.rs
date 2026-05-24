use aghist::export;

use super::load_fixture_session;

#[test]
fn markdown_has_header_and_metadata() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.starts_with("# "), "should start with H1 header");
    assert!(md.contains("**Provider**"), "should have provider metadata");
    assert!(md.contains("**Date**"), "should have date metadata");
    assert!(md.contains("---"), "should have horizontal rule separator");
}

#[test]
fn markdown_has_role_headers() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.contains("## You"), "should have user role header");
    assert!(
        md.contains("## Assistant"),
        "should have assistant role header"
    );
}

#[test]
fn markdown_preserves_code_blocks() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    assert!(md.contains("```"), "should contain fenced code blocks");
}

#[test]
fn markdown_code_fence_outlasts_embedded_backticks() {
    use aghist::model::ContentBlock;

    let (session, mut messages) = super::sample_session();
    messages[0].content = vec![ContentBlock::CodeBlock {
        language: Some("rust\n```oops".to_string()),
        code: "before\n```\nafter".to_string(),
    }];

    let md = export::to_markdown(&session, &messages);

    assert!(
        md.contains("````rustoops\nbefore\n```\nafter\n````"),
        "embedded fence should force a longer outer fence: {md}"
    );
    assert!(
        !md.contains("```oops"),
        "language info should not inject a fence: {md}"
    );
}

#[test]
fn markdown_has_tool_call_sections() {
    let (session, messages) = load_fixture_session();
    let md = export::to_markdown(&session, &messages);

    let has_tool_use = messages
        .iter()
        .flat_map(|m| &m.content)
        .any(|c| matches!(c, aghist::model::ContentBlock::ToolUse(_)));

    if has_tool_use {
        assert!(
            md.contains("<details>"),
            "tool calls should be in details tags"
        );
        assert!(md.contains("Tool:"), "tool call should show tool name");
    }
}

#[test]
fn markdown_escapes_tool_name_inside_html_summary() {
    use aghist::model::{ContentBlock, ToolCall};

    let (session, mut messages) = super::sample_session();
    messages[0].content = vec![ContentBlock::ToolUse(ToolCall {
        id: "tool-1".to_string(),
        name: "Read</summary><script>alert(1)</script>".to_string(),
        arguments: "{}".to_string(),
    })];

    let md = export::to_markdown(&session, &messages);

    assert!(md.contains("Read&lt;/summary&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(
        !md.contains("Read</summary><script>"),
        "tool name should not break out of the generated summary: {md}"
    );
}
