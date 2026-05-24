use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::Value;

use crate::config::RemoteSource;
use crate::model::Provider;
use crate::provider::HistoryProvider;
use crate::query_scope::QueryScope;

use super::protocol::{serialize_response, Response, RpcError, ERR_INVALID_REQUEST, ERR_PARSE};

mod discovery;
mod dispatch;

pub(super) const MAX_REQUEST_LINE_BYTES: usize = 1024 * 1024;

/// Owns the providers + search index for the lifetime of a server run.
pub struct McpServer {
    pub(super) providers: Vec<Box<dyn HistoryProvider>>,
    scope: QueryScope,
}

impl McpServer {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>) -> Self {
        let visible_providers = providers.iter().map(|p| p.provider()).collect();
        Self {
            providers,
            scope: QueryScope::local(visible_providers),
        }
    }

    pub fn new_federated(
        providers: Vec<Box<dyn HistoryProvider>>,
        sources: Vec<RemoteSource>,
        sources_cache_root: Option<PathBuf>,
        visible_providers: HashSet<Provider>,
    ) -> Self {
        Self::new_scoped(
            providers,
            QueryScope::from_parts(visible_providers, sources, sources_cache_root),
        )
    }

    pub fn new_scoped(providers: Vec<Box<dyn HistoryProvider>>, scope: QueryScope) -> Self {
        Self { providers, scope }
    }

    /// Drives the loop reading newline-delimited JSON from `input` and writing
    /// responses to `output`. Returns when stdin reaches EOF.
    pub fn serve<R: BufRead, W: Write>(&self, mut input: R, mut output: W) -> io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let bytes_read = read_bounded_line(&mut input, &mut line)?;
            if bytes_read == 0 {
                break;
            }

            let response = if bytes_read > MAX_REQUEST_LINE_BYTES {
                Some(error_response(
                    ERR_INVALID_REQUEST,
                    format!("request line exceeds {MAX_REQUEST_LINE_BYTES} bytes"),
                ))
            } else {
                match std::str::from_utf8(&line) {
                    Ok(line) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            self.handle_line(trimmed)
                        }
                    }
                    Err(e) => Some(error_response(ERR_PARSE, format!("parse error: {e}"))),
                }
            };

            let Some(json_line) = response else {
                continue;
            };
            output.write_all(json_line.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
        Ok(())
    }
}

fn read_bounded_line<R: BufRead>(input: &mut R, out: &mut Vec<u8>) -> io::Result<usize> {
    let mut total = 0usize;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(total);
        }

        let newline = available.iter().position(|&b| b == b'\n');
        let consume = newline.map_or(available.len(), |idx| idx + 1);
        let remaining = (MAX_REQUEST_LINE_BYTES + 1).saturating_sub(out.len());
        let copy_len = consume.min(remaining);
        let copy = available.get(..copy_len).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "request buffer slice out of range",
            )
        })?;
        out.extend_from_slice(copy);
        input.consume(consume);
        total = total.saturating_add(consume);

        if newline.is_some() {
            return Ok(total);
        }
    }
}

fn error_response(code: i32, message: impl Into<String>) -> String {
    serialize_response(&Response {
        jsonrpc: "2.0",
        id: Value::Null,
        result: None,
        error: Some(RpcError::new(code, message)),
    })
}
