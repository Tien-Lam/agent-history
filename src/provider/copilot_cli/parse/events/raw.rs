use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(super) struct RawEvent {
    pub(super) id: Option<Value>,
    #[serde(rename = "type")]
    pub(super) event_type: Option<Value>,
    pub(super) timestamp: Option<Value>,
    pub(super) content: Option<Value>,
    pub(super) model: Option<Value>,
    #[serde(rename = "toolName")]
    pub(super) tool_name: Option<Value>,
    #[serde(rename = "toolCallId")]
    pub(super) tool_call_id: Option<Value>,
    #[serde(rename = "toolArgs")]
    pub(super) tool_args: Option<Value>,
    pub(super) usage: Option<RawUsage>,
    pub(super) data: Option<RawEventData>,
}

#[derive(Deserialize)]
pub(super) struct RawEventData {
    pub(super) content: Option<Value>,
    #[serde(rename = "toolRequests")]
    pub(super) tool_requests: Option<Vec<RawToolRequest>>,
    #[serde(rename = "toolName")]
    pub(super) tool_name: Option<Value>,
    #[serde(rename = "toolCallId")]
    pub(super) tool_call_id: Option<Value>,
    pub(super) arguments: Option<Value>,
    pub(super) success: Option<Value>,
    pub(super) result: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct RawToolRequest {
    #[serde(rename = "toolCallId")]
    pub(super) tool_call_id: Option<Value>,
    pub(super) name: Option<Value>,
    pub(super) arguments: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct RawUsage {
    #[serde(rename = "inputTokens")]
    pub(super) input_tokens: Option<Value>,
    #[serde(rename = "outputTokens")]
    pub(super) output_tokens: Option<Value>,
}
