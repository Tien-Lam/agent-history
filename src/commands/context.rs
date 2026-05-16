use std::collections::HashSet;

use aghist::cli_error::ErrorEnvelope;
use aghist::output::{CommandKind, OutputMode};
use aghist::{config, provider};

use super::super::cli::FilterArgs;
use super::filtering::resolve_metadata_filter;

#[derive(Clone, Copy)]
pub(crate) struct OutputFlags {
    json: bool,
    ndjson: bool,
}

impl OutputFlags {
    pub(crate) fn new(json: bool, ndjson: bool) -> Self {
        Self { json, ndjson }
    }
}

pub(crate) struct CommandContext {
    config: config::Config,
    providers: Vec<Box<dyn provider::HistoryProvider>>,
    filters: FilterArgs,
    output: OutputFlags,
}

impl CommandContext {
    pub(crate) fn load(
        filters: FilterArgs,
        json: bool,
        ndjson: bool,
    ) -> Result<Self, ErrorEnvelope> {
        let config = load_config()?;
        let providers = detect_enabled_providers(&config);
        Ok(Self {
            config,
            providers,
            filters,
            output: OutputFlags::new(json, ndjson),
        })
    }

    pub(crate) fn providers(&self) -> &[Box<dyn provider::HistoryProvider>] {
        &self.providers
    }

    pub(crate) fn filters(&self) -> &FilterArgs {
        &self.filters
    }

    pub(crate) fn output_mode(&self, kind: CommandKind) -> OutputMode {
        OutputMode::resolve(self.output.json, self.output.ndjson, kind)
    }

    pub(crate) fn metadata_filter_keys(&self) -> Result<Option<HashSet<String>>, ErrorEnvelope> {
        resolve_metadata_filter(&self.filters)
    }

    pub(crate) fn into_mcp_server(self) -> aghist::mcp::McpServer {
        // MCP gets a narrower view than the rest of the CLI: users can hide
        // providers from MCP clients without disabling them locally.
        let exposed = self.config.mcp_exposed_providers();
        let providers = self
            .providers
            .into_iter()
            .filter(|p| exposed.contains(&p.provider()))
            .collect();
        aghist::mcp::McpServer::new_federated(
            providers,
            self.config.sources,
            config::sources_cache_root(),
            exposed,
        )
    }

    pub(crate) fn into_tui_parts(
        self,
    ) -> (Vec<Box<dyn provider::HistoryProvider>>, config::Config) {
        (self.providers, self.config)
    }
}

fn load_config() -> Result<config::Config, ErrorEnvelope> {
    config::Config::try_load().map_err(|e| {
        ErrorEnvelope::new("config-error", format!("{e}"))
            .with_hint("Fix the TOML or set AGHIST_CONFIG to a known-good config file.")
    })
}

fn detect_enabled_providers(config: &config::Config) -> Vec<Box<dyn provider::HistoryProvider>> {
    let enabled = config.enabled_providers();
    provider::detect_all_providers()
        .into_iter()
        .filter(|p| enabled.contains(&p.provider()))
        .collect()
}
