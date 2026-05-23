use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::config::RemoteSource;
use crate::model::Provider;
use crate::provider::HistoryProvider;
use crate::query_scope::QueryScope;

mod discovery;
mod dispatch;

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
    pub fn serve<R: BufRead, W: Write>(&self, input: R, mut output: W) -> io::Result<()> {
        for line in input.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let response = self.handle_line(trimmed);
            if let Some(json_line) = response {
                output.write_all(json_line.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
        Ok(())
    }
}
