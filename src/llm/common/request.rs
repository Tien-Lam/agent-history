use serde::Serialize;

use super::{LlmConfig, LlmError};

const MAX_LLM_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const REQUEST_JSON_OVERHEAD_BYTES: usize = 512;

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
    let estimated_bytes = config
        .model
        .len()
        .saturating_add(system.len())
        .saturating_add(user.len())
        .saturating_add(REQUEST_JSON_OVERHEAD_BYTES);
    if estimated_bytes > MAX_LLM_REQUEST_BYTES {
        return Err(LlmError::RequestTooLarge {
            bytes: estimated_bytes,
            max_bytes: MAX_LLM_REQUEST_BYTES,
        });
    }

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
    let body = serde_json::to_string(&req).map_err(|e| LlmError::Parse(e.to_string()))?;
    if body.len() > MAX_LLM_REQUEST_BYTES {
        return Err(LlmError::RequestTooLarge {
            bytes: body.len(),
            max_bytes: MAX_LLM_REQUEST_BYTES,
        });
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn cfg() -> LlmConfig {
        LlmConfig {
            endpoint: LlmConfig::DEFAULT_ENDPOINT.to_string(),
            api_key: "test-key".to_string(),
            model: "claude-haiku-test".to_string(),
            max_tokens: 1024,
            anthropic_version: LlmConfig::DEFAULT_VERSION.to_string(),
            timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn request_body_rejects_payloads_above_limit() {
        let user = "x".repeat(MAX_LLM_REQUEST_BYTES);
        let err = build_request_body_with_system(&cfg(), "system", &user).unwrap_err();

        match err {
            LlmError::RequestTooLarge { max_bytes, .. } => {
                assert_eq!(max_bytes, MAX_LLM_REQUEST_BYTES);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
