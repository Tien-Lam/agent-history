use aghist::model::{ContentBlock, Message, Provider, Session};
use serde::Serialize;

use super::cases::ProviderCase;

pub fn provider_contract_json(cases: &[ProviderCase]) -> String {
    let summaries: Vec<ProviderContract> = cases.iter().map(provider_contract).collect();
    serde_json::to_string_pretty(&summaries).expect("provider contract serializes as JSON")
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
