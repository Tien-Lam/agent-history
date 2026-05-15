use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP protocol version we negotiate. The 2024-11-05 revision is the most
/// widely supported by current clients (Claude Desktop, mcp-inspector, etc.).
pub const PROTOCOL_VERSION: &str = "2024-11-05";

pub(super) const SERVER_NAME: &str = "aghist";
pub(super) const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) const ERR_PARSE: i32 = -32700;
pub(super) const ERR_INVALID_REQUEST: i32 = -32600;
pub(super) const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub(super) const ERR_INVALID_PARAMS: i32 = -32602;

#[derive(Debug, Deserialize)]
pub(super) struct Request {
    #[serde(default)]
    pub(super) jsonrpc: String,
    /// Absent for notifications. We store the raw `Value` so we can echo it
    /// back without coercing JSON numbers into Rust ints.
    pub(super) id: Option<Value>,
    pub(super) method: String,
    #[serde(default)]
    pub(super) params: Value,
}

#[derive(Debug, Serialize)]
pub(super) struct Response {
    pub(super) jsonrpc: &'static str,
    pub(super) id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub(super) struct RpcError {
    pub(super) code: i32,
    pub(super) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) data: Option<Value>,
}

impl RpcError {
    pub(super) fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

pub(super) fn serialize_response(resp: &Response) -> String {
    serde_json::to_string(resp).unwrap_or_else(|_| {
        // Last-ditch envelope so the client gets *something* parseable.
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"failed to serialize response"}}"#.to_string()
    })
}
