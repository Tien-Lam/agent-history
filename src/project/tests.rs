use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};

use super::*;
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
};

mod aggregate;
mod extraction;
mod files;
mod metadata;

fn ts(year: i32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, 1, 1, hour, 0, 0).unwrap()
}

fn mk_session(
    id: &str,
    project: Option<&str>,
    model: Option<&str>,
    usage: Option<TokenUsage>,
    started: DateTime<Utc>,
    message_count: usize,
) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: project.map(PathBuf::from),
        project_name: project.map(str::to_string),
        git_branch: None,
        started_at: started,
        ended_at: Some(started + chrono::Duration::minutes(5)),
        summary: None,
        model: model.map(str::to_string),
        token_usage: usage,
        message_count,
        source_path: PathBuf::from(format!("/tmp/{id}")),
    }
}

fn assistant(text: &str, when: DateTime<Utc>) -> Message {
    Message {
        id: MessageId(format!("m-{}", when.timestamp())),
        role: Role::Assistant,
        timestamp: when,
        content: vec![ContentBlock::Text(text.into())],
        model: None,
        token_usage: None,
    }
}

fn tool_call(name: &str, args: &str, when: DateTime<Utc>) -> Message {
    Message {
        id: MessageId(format!("t-{}", when.timestamp())),
        role: Role::Assistant,
        timestamp: when,
        content: vec![ContentBlock::ToolUse(ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: args.into(),
        })],
        model: None,
        token_usage: None,
    }
}
