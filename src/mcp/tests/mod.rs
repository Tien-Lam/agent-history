use std::io::Cursor;

use serde_json::Value;

use super::payload::tool_definitions;
use super::protocol::{
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, ERR_PARSE, PROTOCOL_VERSION,
};
use super::resources::{
    parse_aghist_uri, session_uri, session_uri_for_source, turn_uri, turn_uri_for_source, ParsedUri,
};
use super::McpServer;
use crate::model::Provider;
use crate::schema_fragments;

mod protocol;
mod resources;
mod tool_calls;
mod tools;
mod uris;

fn server() -> McpServer {
    McpServer::new(Vec::new())
}

fn run_one(server: &McpServer, request: &str) -> Value {
    let input = format!("{request}\n");
    let mut output = Vec::new();
    server
        .serve(Cursor::new(input.as_bytes()), &mut output)
        .unwrap();
    let line = String::from_utf8(output).unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

fn tool_by_name<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|tool| tool["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("missing tool {name}"))
}
