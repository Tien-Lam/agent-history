use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::string_or_object_field_or_pretty;
use crate::provider::parse_common::{
    nonzero_token_usage, parse_utc, parse_utc_or_now, pretty_json_opt, token_usage_from_options,
    tool_result_block, tool_use_block,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

#[derive(Deserialize)]
struct ProjectsFile {
    projects: HashMap<String, String>,
}

pub(crate) fn load_project_map(base: &Path) -> HashMap<String, String> {
    let path = base.join("projects.json");
    if !path.exists() {
        return HashMap::new();
    }

    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<ProjectsFile>(&s).ok())
        .map(|pf| {
            // Reverse the map: slug -> path
            pf.projects
                .into_iter()
                .map(|(path, slug)| (slug, path))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn build_session_from_file(
    path: &Path,
    project_slug: &str,
    project_map: &HashMap<String, String>,
) -> Option<Session> {
    let data = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&data).ok()?;

    let message_count = raw
        .messages
        .iter()
        .filter(|m| m.msg_type == "user" || m.msg_type == "gemini")
        .count();

    if message_count == 0 {
        return None;
    }

    let started_at = parse_utc(&raw.start_time)?;
    let ended_at = parse_utc(&raw.last_updated);

    let project_path = project_map.get(project_slug).map(PathBuf::from);

    // Get first user message as summary
    let summary = raw.messages.iter().find_map(|m| {
        if m.msg_type == "user" {
            extract_user_text(m).map(|t| t.chars().take(80).collect())
        } else {
            None
        }
    });

    // Get model from first gemini message
    let model = raw.messages.iter().find_map(|m| m.model.clone());

    // Sum tokens
    let (input_total, output_total) = raw.messages.iter().fold((0u64, 0u64), |(inp, out), m| {
        if let Some(ref tokens) = m.tokens {
            (
                inp + tokens.input.unwrap_or(0),
                out + tokens.output.unwrap_or(0),
            )
        } else {
            (inp, out)
        }
    });

    let token_usage = nonzero_token_usage(input_total, output_total, None, None);

    Some(Session {
        id: SessionId(raw.session_id),
        provider: Provider::GeminiCli,
        project_path,
        project_name: Some(project_slug.to_string()),
        git_branch: None,
        started_at,
        ended_at,
        summary,
        model,
        token_usage,
        message_count,
        source_path: path.to_path_buf(),
    })
}

fn extract_user_text(msg: &RawMessage) -> Option<String> {
    match &msg.content {
        RawContent::Text(s) => Some(s.clone()),
        RawContent::Parts(parts) => {
            // Use displayContent if available, otherwise first text part
            if let Some(ref dc) = msg.display_content {
                dc.iter().find_map(|p| p.text.clone())
            } else {
                parts.iter().find_map(|p| p.text.clone())
            }
        }
    }
}

pub(crate) fn load_messages_from_path(path: &Path) -> Result<Vec<Message>, ProviderError> {
    tracing::debug!(path = %path.display(), "loading Gemini CLI messages");
    let data = std::fs::read_to_string(path)?;
    let raw: RawSession = serde_json::from_str(&data)?;
    let messages = convert_messages(&raw.messages);
    tracing::info!(
        path = %path.display(),
        raw_messages = raw.messages.len(),
        parsed_messages = messages.len(),
        "Gemini CLI message loading complete"
    );
    Ok(messages)
}

fn convert_messages(raw_messages: &[RawMessage]) -> Vec<Message> {
    raw_messages.iter().filter_map(convert_message).collect()
}

fn convert_message(msg: &RawMessage) -> Option<Message> {
    let role = raw_role(&msg.msg_type)?;
    let mut content = message_content(msg, role);

    if content.is_empty() {
        return None;
    }

    Some(Message {
        id: MessageId(msg.id.clone().unwrap_or_default()),
        role,
        timestamp: message_timestamp(msg.timestamp.as_deref()),
        content: std::mem::take(&mut content),
        model: msg.model.clone(),
        token_usage: msg.tokens.as_ref().map(|tokens| {
            token_usage_from_options(tokens.input, tokens.output, tokens.cached, None)
        }),
    })
}

fn raw_role(msg_type: &str) -> Option<Role> {
    match msg_type {
        "user" => Some(Role::User),
        "gemini" => Some(Role::Assistant),
        _ => None,
    }
}

fn message_timestamp(raw: Option<&str>) -> DateTime<Utc> {
    parse_utc_or_now(raw)
}

fn message_content(msg: &RawMessage, role: Role) -> Vec<ContentBlock> {
    let mut content = Vec::new();

    let text = message_text(msg, role);
    if !text.is_empty() {
        content.extend(parse_text_with_code_blocks(&text));
    }

    append_thoughts(&mut content, msg.thoughts.as_deref());
    append_tool_calls(&mut content, msg.tool_calls.as_deref());

    content
}

fn message_text(msg: &RawMessage, role: Role) -> String {
    match &msg.content {
        RawContent::Text(s) => s.clone(),
        RawContent::Parts(parts) if role == Role::User => {
            let preferred = msg.display_content.as_deref().unwrap_or(parts);
            text_parts(preferred)
        }
        RawContent::Parts(parts) => text_parts(parts),
    }
}

fn text_parts(parts: &[TextPart]) -> String {
    parts
        .iter()
        .filter_map(|p| p.text.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_thoughts(content: &mut Vec<ContentBlock>, thoughts: Option<&[Thought]>) {
    let Some(thoughts) = thoughts else {
        return;
    };

    for thought in thoughts {
        let desc = thought.description.as_deref().unwrap_or("");
        if !desc.is_empty() {
            content.push(ContentBlock::Thinking(desc.to_string()));
        }
    }
}

fn append_tool_calls(content: &mut Vec<ContentBlock>, tool_calls: Option<&[RawToolCall]>) {
    let Some(tool_calls) = tool_calls else {
        return;
    };

    for tc in tool_calls {
        let id = tc.id.clone().unwrap_or_default();
        content.push(tool_use_block(
            id.clone(),
            tc.name.clone().unwrap_or_else(|| "unknown".to_string()),
            pretty_json_opt(tc.args.as_ref()),
        ));

        append_tool_response(content, tc, id);
    }
}

fn append_tool_response(content: &mut Vec<ContentBlock>, tc: &RawToolCall, id: String) {
    let Some(output) = tc.response.as_ref().map(extract_tool_response_text) else {
        return;
    };

    if !output.is_empty() {
        content.push(tool_result_block(id, tc.error.is_none(), output));
    }
}

#[derive(Deserialize)]
struct RawSession {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "lastUpdated")]
    last_updated: String,
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    content: RawContent,
    #[serde(rename = "displayContent")]
    display_content: Option<Vec<TextPart>>,
    thoughts: Option<Vec<Thought>>,
    tokens: Option<RawTokens>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<RawToolCall>>,
    model: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Text(String),
    Parts(Vec<TextPart>),
}

impl Default for RawContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

#[derive(Deserialize)]
struct TextPart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct Thought {
    description: Option<String>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
}

#[derive(Deserialize)]
struct RawToolCall {
    id: Option<String>,
    name: Option<String>,
    args: Option<serde_json::Value>,
    /// Populated by gemini-cli after the tool has executed. Shape varies -
    /// often `{"output": "..."}` or a free-form provider blob - so we
    /// accept any JSON value and stringify on read.
    response: Option<serde_json::Value>,
    /// Set when the tool execution failed.
    error: Option<serde_json::Value>,
}

fn extract_tool_response_text(v: &serde_json::Value) -> String {
    string_or_object_field_or_pretty(v, &["output", "result", "content", "text"])
}
