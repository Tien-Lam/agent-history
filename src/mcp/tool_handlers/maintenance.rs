use serde_json::{json, Value};

use super::super::args::optional_provider;
use super::super::server::McpServer;

use crate::health::{
    provider_parse_health_check, run_health_checks, run_provider_fidelity, HealthStatus,
};
use crate::indexing::{self, IndexingOptions, UnfilteredIndexScope};

impl McpServer {
    pub(super) fn tool_reindex(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
        let provider_scope = self.provider_scope();

        if let Some(want) = provider_filter {
            if !provider_scope.contains(&want) {
                return Err(format!("provider '{}' is not visible to MCP", want.slug()));
            }
        }

        let outcome = indexing::run_indexing(
            &self.providers,
            self.scope(),
            IndexingOptions {
                provider_filter,
                force,
                unfiltered_scope: UnfilteredIndexScope::VisibleProviders,
            },
        )
        .map_err(|e| e.message)?;

        serde_json::to_value(outcome.summary)
            .map_err(|e| format!("failed to serialize reindex summary: {e}"))
    }

    pub(super) fn tool_health(&self, _args: &Value) -> Value {
        let fidelity = run_provider_fidelity(&self.providers);
        let mut checks = run_health_checks(&self.providers, self.scope());
        if let Some(check) = provider_parse_health_check(&fidelity) {
            checks.push(check);
        }
        let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);
        let summary = json!({
            "ok_count": checks.iter().filter(|c| c.status == HealthStatus::Ok).count(),
            "warn_count": checks.iter().filter(|c| c.status == HealthStatus::Warn).count(),
            "fail_count": checks.iter().filter(|c| c.status == HealthStatus::Fail).count(),
        });
        json!({
            "ok": !any_failed,
            "checks": checks,
            "summary": summary,
            "provider_fidelity": fidelity,
        })
    }
}
