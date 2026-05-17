use std::path::Path;

use aghist::model::{ContentBlock, Message, Provider, Session};
use aghist::provider::registry::provider_from_dirs;
use aghist::provider::HistoryProvider;
use serde::Serialize;
use tempfile::TempDir;

use super::fixtures;

pub struct ProviderCase {
    pub label: &'static str,
    pub provider: Box<dyn HistoryProvider>,
    pub expected_provider: Provider,
    pub expected_sessions: Option<usize>,
    pub expected_messages_per_session: Option<usize>,
}

pub struct GeneratedProviderCases {
    _dirs: Vec<TempDir>,
    cases: Vec<ProviderCase>,
}

impl GeneratedProviderCases {
    pub fn cases(&self) -> &[ProviderCase] {
        &self.cases
    }

    pub fn providers(&self) -> Vec<Provider> {
        self.cases
            .iter()
            .map(|case| case.expected_provider)
            .collect()
    }
}

pub fn missing_dir_provider_cases(base: &Path) -> Vec<ProviderCase> {
    Provider::all()
        .iter()
        .map(|provider| ProviderCase {
            label: provider.slug(),
            provider: provider_from_dirs(*provider, vec![base.join(format!("missing-{provider}"))]),
            expected_provider: *provider,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        })
        .collect()
}

pub fn generated_provider_cases(
    n_sessions: usize,
    messages_per_session: usize,
) -> GeneratedProviderCases {
    let (dirs, providers) = fixtures::all_generated_providers(n_sessions, messages_per_session);
    let cases = providers
        .into_iter()
        .map(|provider| {
            let expected_provider = provider.provider();
            ProviderCase {
                label: provider_label(expected_provider),
                provider,
                expected_provider,
                expected_sessions: Some(n_sessions),
                expected_messages_per_session: Some(messages_per_session),
            }
        })
        .collect();
    GeneratedProviderCases { _dirs: dirs, cases }
}

pub fn assert_missing_dir_discovers_empty(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    assert_eq!(
        sessions.len(),
        case.expected_sessions.unwrap_or(0),
        "{} should discover no sessions for a missing root",
        case.label
    );
}

pub fn assert_discover_load_roundtrip(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    if let Some(expected) = case.expected_sessions {
        assert_eq!(
            sessions.len(),
            expected,
            "{} discovered an unexpected session count",
            case.label
        );
    }

    for session in &sessions {
        assert!(
            !session.id.0.is_empty(),
            "{} discovered a session with an empty id",
            case.label
        );
        assert_eq!(
            session.provider, case.expected_provider,
            "{} discovered a session tagged with the wrong provider",
            case.label
        );
        let messages = case
            .provider
            .load_messages(session)
            .unwrap_or_else(|e| panic!("{} failed to load {}: {e}", case.label, session.id.0));
        if let Some(expected) = case.expected_messages_per_session {
            assert_eq!(
                messages.len(),
                expected,
                "{} loaded an unexpected message count for {}",
                case.label,
                session.id.0
            );
            assert_eq!(
                session.message_count, expected,
                "{} recorded an unexpected message_count for {}",
                case.label, session.id.0
            );
        } else {
            assert!(
                !messages.is_empty(),
                "{} should load messages for {}",
                case.label,
                session.id.0
            );
        }
        assert_eq!(
            session.message_count,
            messages.len(),
            "{} session {} message_count does not match loaded messages",
            case.label,
            session.id.0
        );
        assert_messages_are_well_formed(case, session, &messages);
    }
}

pub fn provider_contract_json(cases: &[ProviderCase]) -> String {
    let summaries: Vec<ProviderContract> = cases.iter().map(provider_contract).collect();
    serde_json::to_string_pretty(&summaries).expect("provider contract serializes as JSON")
}

fn assert_messages_are_well_formed(case: &ProviderCase, session: &Session, messages: &[Message]) {
    for (idx, message) in messages.iter().enumerate() {
        assert!(
            !message.id.0.is_empty(),
            "{} loaded message {} in {} with an empty id",
            case.label,
            idx,
            session.id.0
        );
        assert!(
            !message.content.is_empty(),
            "{} loaded message {} in {} with no content blocks",
            case.label,
            message.id.0,
            session.id.0
        );
    }
}

fn provider_label(provider: Provider) -> &'static str {
    provider.slug()
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
