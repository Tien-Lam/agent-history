use std::collections::HashSet;
use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::model::{Provider, Session};
use aghist::{config, federated, provider, search};

mod embeddings;

pub(crate) fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let started = std::time::Instant::now();
    let config = config::Config::try_load().map_err(|e| {
        ErrorEnvelope::new("config-error", format!("{e}"))
            .with_hint("Fix the TOML or set AGHIST_CONFIG to a known-good config file.")
    })?;
    let enabled = config.enabled_providers();

    if let Some(want) = filter {
        if !enabled.contains(&want) {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!("provider '{}' is not enabled in config", want.slug()),
            )
            .with_hint(
                "Add the provider to `[providers].enabled`, or remove the --provider filter.",
            ));
        }
    }

    let active: Vec<&dyn provider::HistoryProvider> = providers
        .iter()
        .map(Box::as_ref)
        .filter(|p| filter.is_none_or(|want| p.provider() == want))
        .collect();

    let mut sessions: Vec<Session> = Vec::new();
    let mut errors: Vec<(Provider, String)> = Vec::new();
    for p in &active {
        match p.discover_sessions() {
            Ok(s) => sessions.extend(s),
            Err(e) => errors.push((p.provider(), e.to_string())),
        }
    }
    let source_failures = append_remote_sessions(&mut sessions, &config, &enabled, filter);

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open index at {}: {e}", index_dir.display()),
        )
    })?;
    if force {
        index.clear().map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))
        })?;
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    // build_index needs the full provider list for load_messages dispatch;
    // provider filtering is enforced by only feeding it sessions from `active`.
    let stats = index
        .build_index(&sessions, providers, &tx)
        .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to build index: {e}")))?;

    #[cfg(feature = "embeddings")]
    let embed_summary =
        embeddings::run_embeddings(&index_dir, &sessions, providers, accept_download)?;
    #[cfg(not(feature = "embeddings"))]
    let embed_summary = embeddings::disabled_embeddings_summary(accept_download);

    let provider_slugs = provider_slugs(filter, &active, &sessions);
    let summary = serde_json::json!({
        "providers": provider_slugs,
        "sessions_total": sessions.len(),
        "added": stats.added,
        "updated": stats.updated,
        "unchanged": stats.unchanged,
        "removed": stats.removed,
        "messages_indexed": stats.messages_indexed,
        "force": force,
        "index_dir": index_dir.display().to_string(),
        "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "errors": errors
            .iter()
            .map(|(p, msg)| serde_json::json!({ "provider": p.slug(), "error": msg }))
            .chain(source_failures.iter().map(|failure| {
                serde_json::json!({ "source": failure.source, "error": failure.message })
            }))
            .collect::<Vec<_>>(),
        "embeddings": embed_summary,
    });

    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &summary).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write index output: {e}"))
    })?;
    writeln!(out).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write index output: {e}"))
    })?;
    Ok(EXIT_OK)
}

fn append_remote_sessions(
    sessions: &mut Vec<Session>,
    config: &config::Config,
    enabled: &HashSet<Provider>,
    filter: Option<Provider>,
) -> Vec<federated::SourceFailure> {
    let Some(cache_root) = config::sources_cache_root() else {
        return Vec::new();
    };

    let remote = federated::discover_remote_sources(&config.sources, &cache_root);
    sessions.extend(
        remote
            .sessions
            .into_iter()
            .filter(|session| enabled.contains(&session.provider))
            .filter(|session| filter.is_none_or(|want| session.provider == want)),
    );
    remote.failures
}

fn provider_slugs(
    filter: Option<Provider>,
    active: &[&dyn provider::HistoryProvider],
    sessions: &[Session],
) -> Vec<&'static str> {
    if let Some(want) = filter {
        return vec![want.slug()];
    }

    Provider::all()
        .iter()
        .copied()
        .filter(|provider| {
            active.iter().any(|p| p.provider() == *provider)
                || sessions.iter().any(|session| session.provider == *provider)
        })
        .map(Provider::slug)
        .collect()
}
