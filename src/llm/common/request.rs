use serde::Serialize;

use super::{LlmConfig, LlmError};

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    cache_control: CacheControl,
}

#[derive(Serialize)]
struct UserMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    system: [SystemBlock<'a>; 1],
    messages: [UserMessage<'a>; 1],
}

/// Build the JSON request body for an extraction call with a caller-supplied
/// system prompt. All LLM routes share this shape so prompt caching behaves
/// consistently.
pub(in crate::llm) fn build_request_body_with_system(
    config: &LlmConfig,
    system: &str,
    user: &str,
) -> Result<String, LlmError> {
    let req = Request {
        model: &config.model,
        max_tokens: config.max_tokens,
        system: [SystemBlock {
            kind: "text",
            text: system,
            cache_control: CacheControl { kind: "ephemeral" },
        }],
        messages: [UserMessage {
            role: "user",
            content: user,
        }],
    };
    serde_json::to_string(&req).map_err(|e| LlmError::Parse(e.to_string()))
}
