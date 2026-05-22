use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::ProviderError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::json_text::{string_or_object_field_or_pretty, stringish, value_u64};
use crate::provider::parse_common::{
    nonzero_token_usage, pretty_json_opt, timestamp_value_to_utc, token_usage_from_options,
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
    let session_id = stringish(raw.session_id.as_ref(), &["sessionId", "id"]).or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
    })?;

    let message_count = raw
        .messages
        .iter()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .count();

    if message_count == 0 {
        return None;
    }

    let started_at = gemini_timestamp(raw.start_time.as_ref())
        .or_else(|| first_message_timestamp(&raw.messages))?;
    let ended_at = gemini_timestamp(raw.last_updated.as_ref())
        .or_else(|| last_message_timestamp(&raw.messages));

    let project_path = project_map.get(project_slug).map(PathBuf::from);

    // Get first user message as summary
    let summary = raw.messages.iter().find_map(|m| {
        if raw_role(m.msg_type.as_ref()) == Some(Role::User) {
            extract_user_text(m).map(|t| t.chars().take(80).collect())
        } else {
            None
        }
    });

    // Get model from first gemini message
    let model = raw
        .messages
        .iter()
        .find_map(|m| stringish(m.model.as_ref(), &["model", "id", "name"]));

    // Sum tokens
    let (input_total, output_total) = raw.messages.iter().fold((0u64, 0u64), |(inp, out), m| {
        if let Some(ref tokens) = m.tokens {
            (
                inp + value_u64(tokens.input.as_ref()).unwrap_or(0),
                out + value_u64(tokens.output.as_ref()).unwrap_or(0),
            )
        } else {
            (inp, out)
        }
    });

    let token_usage = nonzero_token_usage(input_total, output_total, None, None);

    Some(Session {
        id: SessionId(session_id),
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

fn first_message_timestamp(messages: &[RawMessage]) -> Option<DateTime<Utc>> {
    messages
        .iter()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .find_map(|m| gemini_timestamp(m.timestamp.as_ref()))
}

fn last_message_timestamp(messages: &[RawMessage]) -> Option<DateTime<Utc>> {
    messages
        .iter()
        .rev()
        .filter(|m| raw_role(m.msg_type.as_ref()).is_some())
        .find_map(|m| gemini_timestamp(m.timestamp.as_ref()))
}

fn extract_user_text(msg: &RawMessage) -> Option<String> {
    match &msg.content {
        RawContent::Text(s) => Some(s.clone()),
        RawContent::Parts(parts) => {
            // Use displayContent if available, otherwise first text part
            let text = if let Some(ref dc) = msg.display_content {
                text_parts(dc)
            } else {
                text_parts(parts)
            };
            (!text.is_empty()).then_some(text)
        }
        RawContent::Json(value) => Some(string_or_object_field_or_pretty(
            value,
            &["text", "content", "message"],
        ))
        .filter(|s| !s.is_empty()),
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
    let role = raw_role(msg.msg_type.as_ref())?;
    let mut content = message_content(msg, role);

    if content.is_empty() {
        return None;
    }

    Some(Message {
        id: MessageId(stringish(msg.id.as_ref(), &["id"]).unwrap_or_default()),
        role,
        timestamp: message_timestamp(msg.timestamp.as_ref()),
        content: std::mem::take(&mut content),
        model: stringish(msg.model.as_ref(), &["model", "id", "name"]),
        token_usage: msg.tokens.as_ref().map(|tokens| {
            token_usage_from_options(
                value_u64(tokens.input.as_ref()),
                value_u64(tokens.output.as_ref()),
                value_u64(tokens.cached.as_ref()),
                None,
            )
        }),
    })
}

fn raw_role(msg_type: Option<&Value>) -> Option<Role> {
    match stringish(msg_type, &["type", "role"]).as_deref() {
        Some("user") => Some(Role::User),
        Some("gemini") => Some(Role::Assistant),
        _ => None,
    }
}

fn message_timestamp(raw: Option<&Value>) -> DateTime<Utc> {
    gemini_timestamp(raw).unwrap_or_else(Utc::now)
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
        RawContent::Json(value) => {
            string_or_object_field_or_pretty(value, &["text", "content", "message"])
        }
    }
}

fn text_parts(parts: &[TextPart]) -> String {
    parts
        .iter()
        .filter_map(|p| stringish(p.text.as_ref(), &["text", "content", "message"]))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_thoughts(content: &mut Vec<ContentBlock>, thoughts: Option<&[Thought]>) {
    let Some(thoughts) = thoughts else {
        return;
    };

    for thought in thoughts {
        if let Some(desc) = stringish(
            thought.description.as_ref(),
            &["description", "text", "content"],
        )
        .filter(|desc| !desc.is_empty())
        {
            content.push(ContentBlock::Thinking(desc));
        }
    }
}

fn append_tool_calls(content: &mut Vec<ContentBlock>, tool_calls: Option<&[RawToolCall]>) {
    let Some(tool_calls) = tool_calls else {
        return;
    };

    for tc in tool_calls {
        let id = stringish(tc.id.as_ref(), &["id", "toolCallId"]).unwrap_or_default();
        content.push(tool_use_block(
            id.clone(),
            stringish(tc.name.as_ref(), &["name", "toolName"])
                .unwrap_or_else(|| "unknown".to_string()),
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
    session_id: Option<Value>,
    #[serde(rename = "startTime")]
    start_time: Option<Value>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_vec_skip_invalid")]
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "type")]
    msg_type: Option<Value>,
    #[serde(default)]
    content: RawContent,
    #[serde(rename = "displayContent")]
    #[serde(default, deserialize_with = "deserialize_optional_vec")]
    display_content: Option<Vec<TextPart>>,
    #[serde(default, deserialize_with = "deserialize_optional_vec")]
    thoughts: Option<Vec<Thought>>,
    tokens: Option<RawTokens>,
    #[serde(rename = "toolCalls")]
    #[serde(default, deserialize_with = "deserialize_optional_vec")]
    tool_calls: Option<Vec<RawToolCall>>,
    model: Option<Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Text(String),
    Parts(Vec<TextPart>),
    Json(serde_json::Value),
}

impl Default for RawContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

#[derive(Deserialize)]
struct TextPart {
    text: Option<Value>,
}

#[derive(Deserialize)]
struct Thought {
    description: Option<Value>,
}

#[derive(Deserialize)]
struct RawTokens {
    input: Option<Value>,
    output: Option<Value>,
    cached: Option<Value>,
}

#[derive(Deserialize)]
struct RawToolCall {
    id: Option<Value>,
    name: Option<Value>,
    args: Option<serde_json::Value>,
    /// Populated by gemini-cli after the tool has executed. Shape varies -
    /// often `{"output": "..."}` or a free-form provider blob - so we
    /// accept any JSON value and stringify on read.
    response: Option<serde_json::Value>,
    /// Set when the tool execution failed.
    error: Option<serde_json::Value>,
}

fn gemini_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(value, &["timestamp", "startTime", "lastUpdated", "value"])
}

fn deserialize_vec_skip_invalid<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };

    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };

    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

fn deserialize_optional_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<serde_json::Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    let serde_json::Value::Array(items) = value else {
        return Ok(None);
    };

    let parsed = items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect();
    Ok(Some(parsed))
}

fn extract_tool_response_text(v: &serde_json::Value) -> String {
    string_or_object_field_or_pretty(v, &["output", "result", "content", "text"])
}
