use std::collections::HashSet;

use aghist::cli_error::ErrorEnvelope;
use aghist::output::{CommandKind, OutputMode};
use aghist::{config, provider, query_scope};

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

    fn json_only(self, local_json: bool, command: &str) -> Result<bool, ErrorEnvelope> {
        if self.ndjson {
            if local_json {
                return Err(conflicting_output_flags_error());
            }
            return Err(ErrorEnvelope::new(
                "usage",
                format!("{command} does not support --ndjson"),
            )
            .with_hint("Use --json for a single JSON document, or remove --ndjson."));
        }
        Ok(self.json || local_json)
    }
}

pub(crate) struct CommandContext {
    config: config::Config,
    scope: query_scope::QueryScope,
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
        validate_filter_args(&filters)?;
        let config = load_config()?;
        let scope = query_scope::QueryScope::enabled(&config);
        let providers = query_scope::detect_enabled_providers(&config);
        Ok(Self {
            config,
            scope,
            providers,
            filters,
            output: OutputFlags::new(json, ndjson),
        })
    }

    pub(crate) fn providers(&self) -> &[Box<dyn provider::HistoryProvider>] {
        &self.providers
    }

    pub(crate) fn scope(&self) -> &query_scope::QueryScope {
        &self.scope
    }

    pub(crate) fn filters(&self) -> &FilterArgs {
        &self.filters
    }

    pub(crate) fn output_mode(&self, kind: CommandKind) -> OutputMode {
        OutputMode::resolve(self.output.json, self.output.ndjson, kind)
    }

    pub(crate) fn output_mode_with_local_json(
        &self,
        kind: CommandKind,
        local_json: bool,
    ) -> Result<OutputMode, ErrorEnvelope> {
        output_mode_with_local_json(self.output_mode(kind), local_json)
    }

    pub(crate) fn json_only_output(
        &self,
        local_json: bool,
        command: &str,
    ) -> Result<bool, ErrorEnvelope> {
        self.output.json_only(local_json, command)
    }

    pub(crate) fn metadata_filter_keys(&self) -> Result<Option<HashSet<String>>, ErrorEnvelope> {
        resolve_metadata_filter(&self.filters)
    }

    pub(crate) fn into_mcp_server(self) -> aghist::mcp::McpServer {
        let scope = query_scope::QueryScope::mcp_visible(&self.config);
        let providers = scope.filter_provider_instances(self.providers);
        aghist::mcp::McpServer::new_scoped(providers, scope)
    }

    pub(crate) fn into_tui_parts(
        self,
    ) -> (Vec<Box<dyn provider::HistoryProvider>>, config::Config) {
        (self.providers, self.config)
    }
}

pub(crate) fn output_mode_with_local_json(
    default_mode: OutputMode,
    local_json: bool,
) -> Result<OutputMode, ErrorEnvelope> {
    if local_json && matches!(default_mode, OutputMode::Ndjson) {
        return Err(conflicting_output_flags_error());
    }
    if local_json {
        Ok(OutputMode::Json)
    } else {
        Ok(default_mode)
    }
}

fn conflicting_output_flags_error() -> ErrorEnvelope {
    ErrorEnvelope::new("usage", "--json and --ndjson are mutually exclusive").with_hint(
        "Pick one. Use --json for a single JSON document; use --ndjson for streaming rows.",
    )
}

fn load_config() -> Result<config::Config, ErrorEnvelope> {
    config::Config::try_load().map_err(|e| {
        ErrorEnvelope::new("config-error", format!("{e}"))
            .with_hint("Fix the TOML or set AGHIST_CONFIG to a known-good config file.")
    })
}

fn validate_filter_args(filters: &FilterArgs) -> Result<(), ErrorEnvelope> {
    if let (Some(since), Some(until)) = (filters.since, filters.until) {
        if since > until {
            return Err(ErrorEnvelope::new(
                "usage",
                "--since must be less than or equal to --until",
            )
            .with_hint("Pass timestamps in chronological order."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::OutputFlags;

    #[test]
    fn json_only_output_accepts_global_or_local_json() {
        assert!(OutputFlags::new(true, false)
            .json_only(false, "search")
            .unwrap());
        assert!(OutputFlags::new(false, false)
            .json_only(true, "search")
            .unwrap());
    }

    #[test]
    fn json_only_output_rejects_ndjson() {
        let err = OutputFlags::new(false, true)
            .json_only(false, "search")
            .unwrap_err();
        assert_eq!(err.kind, "usage");
        assert!(err.message.contains("search does not support --ndjson"));
    }

    #[test]
    fn json_only_output_rejects_local_json_with_global_ndjson_as_conflict() {
        let err = OutputFlags::new(false, true)
            .json_only(true, "search")
            .unwrap_err();
        assert_eq!(err.kind, "usage");
        assert_eq!(err.message, "--json and --ndjson are mutually exclusive");
    }

    #[test]
    fn output_mode_with_local_json_preserves_default_without_local_json() {
        assert_eq!(
            super::output_mode_with_local_json(aghist::output::OutputMode::Ndjson, false).unwrap(),
            aghist::output::OutputMode::Ndjson
        );
    }

    #[test]
    fn output_mode_with_local_json_rejects_local_json_with_default_ndjson() {
        let err = super::output_mode_with_local_json(aghist::output::OutputMode::Ndjson, true)
            .unwrap_err();
        assert_eq!(err.kind, "usage");
        assert_eq!(err.message, "--json and --ndjson are mutually exclusive");
    }
}
