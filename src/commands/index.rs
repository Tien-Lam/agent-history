#[cfg(feature = "embeddings")]
use std::{collections::HashSet, path::Path};

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
#[cfg(feature = "embeddings")]
use aghist::embed;
use aghist::model::{Provider, Session};
use aghist::{provider, search};

pub(crate) fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let started = std::time::Instant::now();

    let active: Vec<&Box<dyn provider::HistoryProvider>> = providers
        .iter()
        .filter(|p| filter.is_none_or(|want| p.provider() == want))
        .collect();

    if let Some(want) = filter {
        if active.is_empty() {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "provider '{}' is not enabled or not detected on this system",
                    want.slug()
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table)."));
        }
    }

    let mut sessions: Vec<Session> = Vec::new();
    let mut errors: Vec<(Provider, String)> = Vec::new();
    for p in &active {
        match p.discover_sessions() {
            Ok(s) => sessions.extend(s),
            Err(e) => errors.push((p.provider(), e.to_string())),
        }
    }

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
    let embed_summary = run_embeddings(&index_dir, &sessions, providers, accept_download)?;
    #[cfg(not(feature = "embeddings"))]
    let embed_summary = disabled_embeddings_summary(accept_download);

    let provider_slugs: Vec<&'static str> = active.iter().map(|p| p.provider().slug()).collect();
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
            .collect::<Vec<_>>(),
        "embeddings": embed_summary,
    });

    println!("{summary}");
    Ok(EXIT_OK)
}

/// Drive the (opt-in) semantic side of indexing.
///
/// Three states feed the JSON summary back to the caller:
///
/// - `disabled`: the binary was built without the `embeddings` feature, so we
///   surface that even when `--accept-download` is passed (users would
///   otherwise see silent no-ops).
/// - `awaiting-consent`: feature is compiled in, no consent file exists, and
///   `--accept-download` was not passed. Lexical indexing still happened.
/// - `enabled`: consent recorded (just now or in a prior run); embeddings
///   were generated and persisted.
#[cfg(not(feature = "embeddings"))]
fn disabled_embeddings_summary(accept_download: bool) -> serde_json::Value {
    serde_json::json!({
        "status": "disabled",
        "reason": "binary built without `embeddings` feature",
        "accept_download_requested": accept_download,
    })
}

#[cfg(feature = "embeddings")]
fn run_embeddings(
    index_dir: &std::path::Path,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    let consent = embed::Consent::load(index_dir);
    let consent = match (consent, accept_download) {
        (Some(c), _) => c,
        (None, true) => embed::Consent::record(index_dir, embed::DEFAULT_MODEL).map_err(|e| {
            ErrorEnvelope::new(
                "embed-error",
                format!("failed to record embedding-download consent: {e}"),
            )
        })?,
        (None, false) => {
            return Ok(serde_json::json!({
                "status": "awaiting-consent",
                "model": embed::DEFAULT_MODEL,
                "hint": "re-run with `--accept-download` to enable semantic indexing",
            }));
        }
    };

    let (mut store, evicted_old_schema) = open_embedding_store(index_dir)?;

    let mut errors: Vec<String> = Vec::new();
    let mut messages_embedded = 0usize;
    let mut messages_reused = 0usize;
    let mut live_keys = HashSet::new();
    let mut skipped_prune_due_to_load_error = false;
    let cache_dir = index_dir.join("models");
    let mut embedder: Option<embed::Embedder> = None;

    for session in sessions {
        let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) else {
            continue;
        };
        let messages = match provider.load_messages(session) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: {e}", session.id.0));
                skipped_prune_due_to_load_error = true;
                continue;
            }
        };

        let pending = pending_embeddings(
            session,
            &messages,
            &store,
            &mut live_keys,
            &mut messages_reused,
        );

        if pending.is_empty() {
            continue;
        }

        let texts: Vec<String> = pending.iter().map(|(_, t, _)| t.clone()).collect();
        if embedder.is_none() {
            embedder = Some(embed::Embedder::try_new(&cache_dir).map_err(|e| {
                ErrorEnvelope::new("embed-error", format!("failed to initialise embedder: {e}"))
            })?);
        }
        let Some(embedder) = embedder.as_mut() else {
            unreachable!("embedder was initialised above");
        };
        match embedder.embed_batch(&texts) {
            Ok(vectors) => {
                for ((message_key, _, hash), vec) in pending.into_iter().zip(vectors) {
                    if let Err(e) = store.upsert(&message_key, hash, vec) {
                        errors.push(format!("{message_key}: {e}"));
                    } else {
                        messages_embedded += 1;
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {e}", session.id.0)),
        }
    }

    let messages_pruned_from_store = if skipped_prune_due_to_load_error {
        0
    } else {
        store.retain_keys(&live_keys)
    };

    store.flush().map_err(|e| {
        ErrorEnvelope::new("embed-error", format!("failed to persist embeddings: {e}"))
    })?;

    Ok(serde_json::json!({
        "status": "enabled",
        "model": consent.model,
        "dim": store.dim(),
        "messages_embedded": messages_embedded,
        "messages_reused_from_cache": messages_reused,
        "messages_pruned_from_store": messages_pruned_from_store,
        "messages_total_in_store": store.len(),
        "evicted_old_schema": evicted_old_schema,
        "consent_accepted_at": consent.accepted_at,
        "errors": errors,
    }))
}

#[cfg(feature = "embeddings")]
type PendingEmbedding = (String, String, [u8; embed::HASH_LEN]);

#[cfg(feature = "embeddings")]
fn open_embedding_store(index_dir: &Path) -> Result<(embed::EmbeddingStore, bool), ErrorEnvelope> {
    // On a schema bump (STORE_VERSION mismatch), evict the old sidecar and
    // start fresh. Refusing to reindex would be worse UX than transparently
    // rebuilding, and the JSON summary still surfaces the eviction.
    match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(store)) => Ok((store, false)),
        Ok(None) => Ok((
            embed::EmbeddingStore::create(index_dir, embed::DEFAULT_MODEL, embed::DEFAULT_DIM),
            false,
        )),
        Err(embed::EmbedError::SchemaMismatch { .. }) => {
            embed::EmbeddingStore::evict(index_dir).map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to evict outdated embedding store: {e}"),
                )
            })?;
            Ok((
                embed::EmbeddingStore::create(index_dir, embed::DEFAULT_MODEL, embed::DEFAULT_DIM),
                true,
            ))
        }
        Err(e) => Err(ErrorEnvelope::new(
            "embed-error",
            format!("failed to open embedding store: {e}"),
        )),
    }
}

#[cfg(feature = "embeddings")]
fn pending_embeddings(
    session: &Session,
    messages: &[aghist::model::Message],
    store: &embed::EmbeddingStore,
    live_keys: &mut HashSet<String>,
    messages_reused: &mut usize,
) -> Vec<PendingEmbedding> {
    // (id, text, content_hash) for messages whose cached vector is stale or
    // absent. We compute the hash up front so the freshness check is a cheap
    // byte compare against what's in the store.
    messages
        .iter()
        .enumerate()
        .filter_map(|(turn_index, m)| {
            let text = collect_text(m);
            if text.trim().is_empty() {
                return None;
            }
            let hash = embed::content_hash(&text);
            let message_key = session.message_key(turn_index, &m.id.0);
            live_keys.insert(message_key.clone());
            if store.get_if_fresh(&message_key, &hash).is_some() {
                *messages_reused += 1;
                return None;
            }
            Some((message_key, text, hash))
        })
        .collect()
}

#[cfg(feature = "embeddings")]
fn collect_text(message: &aghist::model::Message) -> String {
    use aghist::model::ContentBlock;
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}
