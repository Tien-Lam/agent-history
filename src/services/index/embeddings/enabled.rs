use std::{collections::HashSet, path::Path};

use crate::cli_error::ErrorEnvelope;
use crate::model::Session;
use crate::{embed, provider};

mod pending;
mod store;

use pending::pending_embeddings;
use store::open_embedding_store;

pub(in crate::services::index) fn run_embeddings(
    index_dir: &Path,
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
            return Err(ErrorEnvelope::new(
                "embed-error",
                "embedder was not available after initialisation",
            ));
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
