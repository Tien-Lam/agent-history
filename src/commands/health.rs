use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_ERROR, EXIT_OK};
use aghist::health::{self, HealthCheck, HealthStatus};
use aghist::output::{write_json_line, OutputMode};
use aghist::provider;
use aghist::provider_diagnostic::ProviderDiagnostic;

pub(crate) fn health_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let checks = health::run_health_checks(providers);
    let fidelity = health::run_provider_fidelity(providers);
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_health_human(&mut out, &checks, &fidelity),
        OutputMode::Json | OutputMode::Ndjson => {
            render_health_json(&mut out, &checks, &fidelity, !any_failed)
        }
    }
    .map_err(|e| ErrorEnvelope::io("failed to write health output", e))?;

    Ok(if any_failed { EXIT_ERROR } else { EXIT_OK })
}

fn render_health_human<W: io::Write>(
    out: &mut W,
    checks: &[HealthCheck],
    fidelity: &[ProviderDiagnostic],
) -> io::Result<()> {
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);
    let any_warn = checks.iter().any(|c| c.status == HealthStatus::Warn);
    let summary = if any_failed {
        "FAIL"
    } else if any_warn {
        "WARN"
    } else {
        "OK"
    };
    writeln!(out, "Overall: {summary}")?;
    writeln!(out)?;
    for c in checks {
        let tag = match c.status {
            HealthStatus::Ok => "OK  ",
            HealthStatus::Warn => "WARN",
            HealthStatus::Fail => "FAIL",
        };
        writeln!(out, "  [{tag}] {} — {}", c.name, c.message)?;
        if let Some(hint) = &c.hint {
            writeln!(out, "         hint: {hint}")?;
        }
    }
    if !fidelity.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "Provider fidelity (sample of up to {} sessions per provider):",
            health::HEALTH_FIDELITY_SAMPLE_PER_PROVIDER,
        )?;
        for d in fidelity {
            let f = &d.tool_call_fidelity;
            writeln!(
                out,
                "  {} ({}): sessions={} messages={} tool_calls={} paired={} unpaired={} orphan_results={} empty_names={}",
                d.label,
                d.provider,
                d.session_count,
                d.message_count,
                f.tool_calls,
                f.paired,
                f.unpaired_calls,
                f.orphan_results,
                f.empty_names,
            )?;
        }
    }
    Ok(())
}

fn render_health_json<W: io::Write>(
    out: &mut W,
    checks: &[HealthCheck],
    fidelity: &[ProviderDiagnostic],
    ok: bool,
) -> io::Result<()> {
    let summary = serde_json::json!({
        "ok_count": checks.iter().filter(|c| c.status == HealthStatus::Ok).count(),
        "warn_count": checks.iter().filter(|c| c.status == HealthStatus::Warn).count(),
        "fail_count": checks.iter().filter(|c| c.status == HealthStatus::Fail).count(),
    });
    let payload = serde_json::json!({
        "ok": ok,
        "checks": checks,
        "summary": summary,
        "provider_fidelity": fidelity,
    });
    write_json_line(out, &payload)
}
