use serde_json::Value;

use crate::common;
use crate::common::mcp::run_session;

#[test]
fn mcp_tool_calls_do_not_mutate_provider_history() {
    // Read-only contract: every tool exposed by the MCP server must leave the
    // upstream session files untouched. We snapshot every file under the
    // provider's base directory before/after a representative battery of tool
    // calls and assert byte-for-byte equality.
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();

    let before = common::mcp::snapshot_tree(&fixture.base_path);

    let responses = run_session(
        home,
        &[
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            serde_json::json!({
                "jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"name":"list_sessions","arguments":{"provider":"claude-code"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"search_sessions","arguments":{"query":"User"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":4,"method":"tools/call",
                "params":{"name":"get_session","arguments":{"session_id":"session-gen"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":5,"method":"tools/call",
                "params":{"name":"get_message","arguments":{"ref":"claude-code/session-gen#1"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":6,"method":"tools/call",
                "params":{"name":"reindex","arguments":{"force":true}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":7,"method":"tools/call",
                "params":{"name":"health","arguments":{}}
            }),
            serde_json::json!({"jsonrpc":"2.0","id":8,"method":"resources/list"}),
            serde_json::json!({
                "jsonrpc":"2.0","id":9,"method":"resources/read",
                "params":{"uri":"aghist://session/claude-code/session-gen"}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":10,"method":"resources/read",
                "params":{"uri":"aghist://session/claude-code/session-gen/turn/1"}
            }),
        ],
    );
    // Every call should have succeeded — otherwise we'd be asserting "no
    // mutation" on a code path that never ran.
    for resp in &responses {
        if let Some(result) = resp.get("result") {
            if let Some(is_error) = result.get("isError") {
                assert_eq!(is_error, &Value::Bool(false), "tool errored: {resp}");
            }
        }
    }

    let after = common::mcp::snapshot_tree(&fixture.base_path);
    assert_eq!(
        before, after,
        "MCP tool calls mutated provider history files (read-only contract violated)"
    );
}
