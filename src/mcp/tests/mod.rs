use std::io::Cursor;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::Value;

use super::payload::tool_definitions;
use super::protocol::{
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE, PROTOCOL_VERSION,
};
use super::resources::{
    parse_aghist_uri, session_uri, session_uri_for_source, turn_uri, turn_uri_for_source, ParsedUri,
};
use super::McpServer;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::schema_fragments;

mod protocol;
mod resources;
mod tool_calls;
mod tools;
mod uris;

fn server() -> McpServer {
    McpServer::new(Vec::new())
}

fn server_with_fake_sessions(session_count: usize, message_count: usize) -> McpServer {
    let sessions = (0..session_count)
        .map(|idx| fake_session(&format!("fake-session-{idx}"), message_count, idx))
        .collect();
    McpServer::new(vec![Box::new(FakeProvider {
        sessions,
        messages: fake_messages(message_count),
    })])
}

fn server_with_fake_session(message_count: usize) -> McpServer {
    let session = fake_session("fake-session", message_count, 0);
    McpServer::new(vec![Box::new(FakeProvider {
        sessions: vec![session],
        messages: fake_messages(message_count),
    })])
}

struct FakeProvider {
    sessions: Vec<Session>,
    messages: Vec<Message>,
}

impl HistoryProvider for FakeProvider {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &[]
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        Ok(self.sessions.clone())
    }

    fn load_messages(&self, _session: &Session) -> Result<Vec<Message>, ProviderError> {
        Ok(self.messages.clone())
    }

    fn load_messages_with_stats(
        &self,
        _session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        Ok(ProviderMessageLoad::from_messages(self.messages.clone()))
    }
}

fn fake_session(id: &str, message_count: usize, offset_seconds: usize) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: None,
        project_name: Some("fake-project".to_string()),
        git_branch: None,
        started_at: Utc
            .timestamp_opt(1_767_225_600 + i64::try_from(offset_seconds).unwrap(), 0)
            .single()
            .unwrap(),
        ended_at: None,
        summary: Some(id.to_string()),
        model: None,
        token_usage: None,
        message_count,
        source_path: PathBuf::from(format!("/tmp/{id}.jsonl")),
    }
}

fn fake_messages(count: usize) -> Vec<Message> {
    (0..count)
        .map(|idx| Message {
            id: MessageId(format!("msg-{idx}")),
            role: Role::User,
            timestamp: Utc
                .timestamp_opt(1_767_225_600 + i64::try_from(idx).unwrap(), 0)
                .single()
                .unwrap(),
            content: vec![ContentBlock::Text(format!("message {idx}"))],
            model: None,
            token_usage: None,
        })
        .collect()
}

fn run_one(server: &McpServer, request: &str) -> Value {
    let input = format!("{request}\n");
    let mut output = Vec::new();
    server
        .serve(Cursor::new(input.as_bytes()), &mut output)
        .unwrap();
    let line = String::from_utf8(output).unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

fn tool_by_name<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|tool| tool["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("missing tool {name}"))
}
