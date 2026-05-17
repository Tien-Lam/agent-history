use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{project_name_from_path, ProviderError};
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::string_or_object_field;
use crate::provider::parse_common::{
    parse_utc, parse_utc_or_now, pretty_json_opt, token_usage, tool_result_block, tool_use_block,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Deserialize)]
struct WorkspaceYaml {
    id: Option<String>,
    cwd: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub(crate) fn build_session(session_dir: &Path, workspace_path: &Path) -> Option<Session> {
    let yaml_content = std::fs::read_to_string(workspace_path).ok()?;
    let workspace: WorkspaceYaml = serde_yaml_ng::from_str(&yaml_content).ok()?;

    let session_id = workspace.id.or_else(|| {
        session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    })?;

    let started_at = workspace.created_at.as_deref().and_then(parse_utc)?;

    let ended_at = workspace.updated_at.as_deref().and_then(parse_utc);

    let project_name = workspace.cwd.as_deref().and_then(project_name_from_path);
    let project_path = workspace.cwd.map(PathBuf::from);

    // Count events to estimate message count
    let events_path = session_dir.join("events.jsonl");
    let message_count = if events_path.exists() {
        count_message_events(&events_path)
    } else {
        0
    };

    Some(Session {
        id: SessionId(session_id),
        provider: Provider::CopilotCli,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: None,
        model: None,
        token_usage: None,
        message_count,
        source_path: session_dir.to_path_buf(),
    })
}

fn count_message_events(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let reader = BufReader::new(file);
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| {
            l.contains("\"user.message\"")
                || l.contains("\"assistant.message\"")
                || l.contains("\"tool.execution_start\"")
                || l.contains("\"tool.execution_complete\"")
                || l.contains("\"tool.invoke\"")
                || l.contains("\"tool.result\"")
        })
        .count()
}

pub(crate) fn parse_events_jsonl(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Copilot CLI messages");
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut line_count: usize = 0;
    let mut parse_errors: usize = 0;
    let mut skipped_types: usize = 0;
    let mut empty_content: usize = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        line_count += 1;

        let event: RawEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                parse_errors += 1;
                tracing::warn!(line_num = line_count, error = %e, "failed to parse JSONL line");
                continue;
            }
        };

        let event_type_str = event.event_type.as_deref().unwrap_or("");

        let role = match event_type_str {
            t if t.contains("user") => Role::User,
            t if t.contains("assistant.message") => Role::Assistant,
            "tool.execution_start" => {
                push_tool_execution_start(&mut messages, &event);
                continue;
            }
            "tool.execution_complete" | "tool.result" => {
                push_tool_result(&mut messages, &event);
                continue;
            }
            t if t.contains("tool") => Role::Tool,
            _ => {
                skipped_types += 1;
                tracing::trace!(event_type = event_type_str, "skipping non-message event");
                continue;
            }
        };

        let timestamp = event_timestamp(&event);
        let content = event_content(&event);

        if content.is_empty() {
            empty_content += 1;
            tracing::debug!(
                event_type = event_type_str,
                has_data = event.data.is_some(),
                data_has_content = event.data.as_ref().is_some_and(|d| d.content.is_some()),
                "skipping event with empty content"
            );
            continue;
        }

        messages.push(event_message(&event, role, timestamp, content));
    }

    tracing::info!(
        path = %path.display(),
        lines = line_count,
        parse_errors,
        skipped_types,
        empty_content,
        messages = messages.len(),
        "Copilot CLI message loading complete"
    );

    Ok(messages)
}

fn event_timestamp(event: &RawEvent) -> DateTime<Utc> {
    parse_utc_or_now(event.timestamp.as_deref())
}

fn event_message(
    event: &RawEvent,
    role: Role,
    timestamp: DateTime<Utc>,
    content: Vec<ContentBlock>,
) -> Message {
    Message {
        id: MessageId(event.id.clone().unwrap_or_default()),
        role,
        timestamp,
        content,
        model: event.model.clone(),
        token_usage: event.usage.as_ref().map(|u| {
            token_usage(
                u.input_tokens.unwrap_or(0),
                u.output_tokens.unwrap_or(0),
                None,
                None,
            )
        }),
    }
}

fn tool_message(event: &RawEvent, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(event.id.clone().unwrap_or_default()),
        role: Role::Tool,
        timestamp: event_timestamp(event),
        content,
        model: None,
        token_usage: None,
    }
}

fn push_tool_execution_start(messages: &mut Vec<Message>, event: &RawEvent) {
    let Some(data) = event.data.as_ref() else {
        return;
    };
    messages.push(tool_message(
        event,
        vec![tool_use_block(
            data.tool_call_id.clone().unwrap_or_default(),
            data.tool_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(data.arguments.as_ref()),
        )],
    ));
}

fn push_tool_result(messages: &mut Vec<Message>, event: &RawEvent) {
    let Some(data) = event.data.as_ref() else {
        return;
    };
    let output = data
        .result
        .as_ref()
        .map(extract_result_text)
        .unwrap_or_default();
    if output.is_empty() {
        return;
    }
    messages.push(tool_message(
        event,
        vec![tool_result_block(
            data.tool_call_id.clone().unwrap_or_default(),
            data.success.unwrap_or(true),
            output,
        )],
    ));
}

fn event_content(event: &RawEvent) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    let text = event
        .content
        .as_deref()
        .or_else(|| event.data.as_ref().and_then(|d| d.content.as_deref()));
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        content.extend(parse_text_with_code_blocks(text));
    }
    push_top_level_tool_use(&mut content, event);
    push_nested_tool_requests(&mut content, event.data.as_ref());
    content
}

fn push_top_level_tool_use(content: &mut Vec<ContentBlock>, event: &RawEvent) {
    if let Some(tool_name) = &event.tool_name {
        content.push(tool_use_block(
            event.tool_call_id.clone().unwrap_or_default(),
            tool_name.clone(),
            pretty_json_opt(event.tool_args.as_ref()),
        ));
    }
}

fn push_nested_tool_requests(content: &mut Vec<ContentBlock>, data: Option<&RawEventData>) {
    let Some(tool_requests) = data.and_then(|d| d.tool_requests.as_ref()) else {
        return;
    };
    for tr in tool_requests {
        content.push(tool_use_block(
            tr.tool_call_id.clone().unwrap_or_default(),
            tr.name.clone().unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(tr.arguments.as_ref()),
        ));
    }
}

pub(crate) fn parse_checkpoint_md(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty()
        || content
            .lines()
            .all(|l| l.starts_with('#') || l.starts_with('|') || l.trim().is_empty())
    {
        return Ok(Vec::new());
    }

    Ok(vec![Message {
        id: MessageId("checkpoint".to_string()),
        role: Role::System,
        timestamp: Utc::now(),
        content: parse_text_with_code_blocks(&content),
        model: None,
        token_usage: None,
    }])
}

#[derive(Deserialize)]
struct RawEvent {
    id: Option<String>,
    #[serde(rename = "type")]
    event_type: Option<String>,
    timestamp: Option<String>,
    content: Option<String>,
    model: Option<String>,
    #[serde(rename = "toolName")]
    tool_name: Option<String>,
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    #[serde(rename = "toolArgs")]
    tool_args: Option<serde_json::Value>,
    usage: Option<RawUsage>,
    /// Newer Copilot format nests content inside a `data` object
    data: Option<RawEventData>,
}

#[derive(Deserialize)]
struct RawEventData {
    content: Option<String>,
    #[serde(rename = "toolRequests")]
    tool_requests: Option<Vec<RawToolRequest>>,
    /// `tool.execution_start` fields
    #[serde(rename = "toolName")]
    tool_name: Option<String>,
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    arguments: Option<serde_json::Value>,
    /// `tool.execution_complete` fields
    success: Option<bool>,
    /// `tool.execution_complete`: object with content/detailedContent.
    /// `tool.result` (newer): a plain string. Both shapes are accepted.
    result: Option<serde_json::Value>,
}

fn extract_result_text(v: &serde_json::Value) -> String {
    string_or_object_field(v, &["detailedContent", "content"])
}

#[derive(Deserialize)]
struct RawToolRequest {
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    name: Option<String>,
    arguments: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<u64>,
}
