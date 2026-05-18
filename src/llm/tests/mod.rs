use super::*;
use std::time::Duration;

pub(super) fn cfg() -> LlmConfig {
    LlmConfig {
        endpoint: "https://example.test/v1/messages".into(),
        api_key: "sk-test".into(),
        model: "claude-haiku-test".into(),
        max_tokens: 256,
        anthropic_version: "2023-06-01".into(),
        timeout: Duration::from_secs(5),
    }
}

pub(super) fn assistant_response(decisions_json: &str) -> String {
    format!(
        r#"{{"id":"msg_x","type":"message","role":"assistant","content":[{{"type":"text","text":{}}}],"model":"claude-haiku-test","stop_reason":"end_turn"}}"#,
        serde_json::to_string(decisions_json).unwrap()
    )
}

mod decisions;
mod threads;
mod todos;
