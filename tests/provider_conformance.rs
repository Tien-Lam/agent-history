mod common;

use aghist::model::{ContentBlock, Message, Provider, Session};
use common::provider_conformance::{
    assert_discover_load_roundtrip, assert_missing_dir_discovers_empty, generated_provider_cases,
    missing_dir_provider_cases, ProviderCase,
};
use serde::Serialize;

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
    let (_dirs, cases) = generated_provider_cases(1, 1);
    let providers: Vec<Provider> = cases.iter().map(|case| case.expected_provider).collect();
    assert_eq!(providers, Provider::all());
}

#[test]
fn generated_providers_discover_and_load_messages() {
    let (_dirs, cases) = generated_provider_cases(2, 4);
    for case in &cases {
        assert_discover_load_roundtrip(case);
    }
}

#[test]
fn generated_provider_contract_snapshot() {
    let (_dirs, cases) = generated_provider_cases(1, 3);
    let summaries: Vec<ProviderContract> = cases.iter().map(provider_contract).collect();
    let pretty = serde_json::to_string_pretty(&summaries).unwrap();
    insta::assert_snapshot!("generated_provider_contract", pretty);
}

#[derive(Serialize)]
struct ProviderContract {
    provider: &'static str,
    sessions: Vec<SessionContract>,
}

#[derive(Serialize)]
struct SessionContract {
    id: String,
    provider: &'static str,
    project: Option<String>,
    branch: Option<String>,
    summary: Option<String>,
    model: Option<String>,
    started_at: String,
    message_count: usize,
    messages: Vec<MessageContract>,
}

#[derive(Serialize)]
struct MessageContract {
    role: &'static str,
    model: Option<String>,
    block_kinds: Vec<&'static str>,
    text_preview: Option<String>,
}

fn provider_contract(case: &ProviderCase) -> ProviderContract {
    let mut sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    sessions.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    ProviderContract {
        provider: case.expected_provider.slug(),
        sessions: sessions
            .iter()
            .map(|session| session_contract(case, session))
            .collect(),
    }
}

fn session_contract(case: &ProviderCase, session: &Session) -> SessionContract {
    let messages = case
        .provider
        .load_messages(session)
        .unwrap_or_else(|e| panic!("{} failed to load {}: {e}", case.label, session.id.0));
    SessionContract {
        id: normalized_session_id(session),
        provider: session.provider.slug(),
        project: session.project_name.clone(),
        branch: session.git_branch.clone(),
        summary: session.summary.clone(),
        model: session.model.clone(),
        started_at: session.started_at.to_rfc3339(),
        message_count: session.message_count,
        messages: messages.iter().map(message_contract).collect(),
    }
}

fn normalized_session_id(session: &Session) -> String {
    if session.provider == Provider::Aider {
        return session.id.0.split_once(':').map_or_else(
            || session.id.0.clone(),
            |(_, suffix)| format!("<project-hash>:{suffix}"),
        );
    }
    session.id.0.clone()
}

fn message_contract(message: &Message) -> MessageContract {
    MessageContract {
        role: message.role.slug(),
        model: message.model.clone(),
        block_kinds: message.content.iter().map(content_block_kind).collect(),
        text_preview: message.content.first().map(content_preview),
    }
}

fn content_block_kind(block: &ContentBlock) -> &'static str {
    match block {
        ContentBlock::Text(_) => "text",
        ContentBlock::CodeBlock { .. } => "code_block",
        ContentBlock::ToolUse(_) => "tool_use",
        ContentBlock::ToolResult(_) => "tool_result",
        ContentBlock::Thinking(_) => "thinking",
        ContentBlock::Error(_) => "error",
    }
}

fn content_preview(block: &ContentBlock) -> String {
    let text = match block {
        ContentBlock::Text(text) | ContentBlock::Thinking(text) | ContentBlock::Error(text) => {
            text.as_str()
        }
        ContentBlock::CodeBlock { code, .. } => code.as_str(),
        ContentBlock::ToolUse(call) => call.name.as_str(),
        ContentBlock::ToolResult(result) => result.output.as_str(),
    };
    text.chars().take(80).collect()
}
