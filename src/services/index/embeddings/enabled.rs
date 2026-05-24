use std::{collections::HashSet, path::Path};

use crate::cli_error::ErrorEnvelope;
use crate::model::{Provider, Session};
use crate::{embed, provider};

mod pending;
mod store;

use pending::pending_embeddings;
use store::open_embedding_store;

pub(in crate::services::index) fn run_embeddings(
    index_dir: &Path,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    prune_providers: Option<&HashSet<Provider>>,
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    let Some(consent) = load_or_record_consent(index_dir, accept_download)? else {
        return Ok(serde_json::json!({
            "status": "awaiting-consent",
            "model": embed::DEFAULT_MODEL,
            "hint": "re-run with `--accept-download` to enable semantic indexing",
        }));
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
        let messages = match provider::load_messages_for_session(session, providers) {
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

    let messages_pruned_from_store = match (skipped_prune_due_to_load_error, prune_providers) {
        (true, _) => 0,
        (false, Some(providers)) => store.retain_scoped_keys(&live_keys, |key| {
            message_key_matches_provider_scope(key, providers)
        }),
        (false, None) => store.retain_keys(&live_keys),
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

fn load_or_record_consent(
    index_dir: &Path,
    accept_download: bool,
) -> Result<Option<embed::Consent>, ErrorEnvelope> {
    let consent = match embed::Consent::read(index_dir) {
        Ok(consent) => consent,
        Err(embed::EmbedError::Json(_)) if accept_download => None,
        Err(error) => {
            return Err(ErrorEnvelope::new(
                "embed-error",
                format!("failed to read embedding-download consent: {error}"),
            )
            .with_hint(
                "Delete embeddings-consent.json or re-run `aghist index --accept-download`.",
            ));
        }
    };

    match (consent, accept_download) {
        (Some(consent), _) => Ok(Some(consent)),
        (None, true) => embed::Consent::record(index_dir, embed::DEFAULT_MODEL)
            .map(Some)
            .map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to record embedding-download consent: {e}"),
                )
            }),
        (None, false) => Ok(None),
    }
}

fn message_key_matches_provider_scope(key: &str, providers: &HashSet<Provider>) -> bool {
    let provider_slug = key.split('\x1f').next().unwrap_or("");
    Provider::from_slug(provider_slug).is_some_and(|provider| providers.contains(&provider))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use chrono::TimeZone as _;

    use super::*;
    use crate::model::{Provider, Session, SessionId};

    #[test]
    fn remote_only_sessions_reuse_fresh_vectors_without_pruning() {
        let index_dir = tempfile::tempdir().unwrap();
        embed::Consent::record(index_dir.path(), embed::DEFAULT_MODEL).unwrap();

        let session = Session {
            id: SessionId("session-abc123".to_string()),
            provider: Provider::ClaudeCode,
            project_path: Some(PathBuf::from("/tmp/test-project")),
            project_name: Some("test-project".to_string()),
            git_branch: None,
            started_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 0,
            source_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/projects/test-project/session-abc123.jsonl"),
        };
        let messages = provider::load_messages_for_session(&session, &[]).unwrap();
        let mut store = embed::EmbeddingStore::create(
            index_dir.path(),
            embed::DEFAULT_MODEL,
            embed::DEFAULT_DIM,
        );
        let mut live_keys = HashSet::new();
        let mut reused = 0;
        for (message_key, _, hash) in
            pending_embeddings(&session, &messages, &store, &mut live_keys, &mut reused)
        {
            store
                .upsert(&message_key, hash, vec![0.0; embed::DEFAULT_DIM as usize])
                .unwrap();
        }
        store.flush().unwrap();

        let summary = run_embeddings(index_dir.path(), &[session], &[], None, false).unwrap();

        assert_eq!(summary["errors"].as_array().unwrap().len(), 0);
        assert!(summary["messages_reused_from_cache"].as_u64().unwrap() > 0);
        assert_eq!(summary["messages_pruned_from_store"], 0);
        assert!(summary["messages_total_in_store"].as_u64().unwrap() > 0);
    }

    #[test]
    fn corrupt_consent_without_accept_download_is_reported() {
        let index_dir = tempfile::tempdir().unwrap();
        std::fs::write(embed::Consent::path(index_dir.path()), b"not json").unwrap();

        let err = run_embeddings(index_dir.path(), &[], &[], None, false).unwrap_err();

        assert_eq!(err.kind, "embed-error");
        assert!(err
            .message
            .contains("failed to read embedding-download consent"));
        assert!(err.hint.as_deref().is_some_and(|hint| {
            hint.contains("embeddings-consent.json") && hint.contains("index --accept-download")
        }));
    }

    #[test]
    fn corrupt_consent_with_accept_download_is_refreshed() {
        let index_dir = tempfile::tempdir().unwrap();
        std::fs::write(embed::Consent::path(index_dir.path()), b"not json").unwrap();

        let summary = run_embeddings(index_dir.path(), &[], &[], None, true).unwrap();

        assert_eq!(summary["status"], "enabled");
        assert!(embed::Consent::read(index_dir.path()).unwrap().is_some());
    }

    #[test]
    fn provider_scoped_runs_do_not_prune_other_provider_vectors() {
        let index_dir = tempfile::tempdir().unwrap();
        embed::Consent::record(index_dir.path(), embed::DEFAULT_MODEL).unwrap();

        let session = Session {
            id: SessionId("session-abc123".to_string()),
            provider: Provider::ClaudeCode,
            project_path: Some(PathBuf::from("/tmp/test-project")),
            project_name: Some("test-project".to_string()),
            git_branch: None,
            started_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 0,
            source_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/projects/test-project/session-abc123.jsonl"),
        };
        let messages = provider::load_messages_for_session(&session, &[]).unwrap();
        let unrelated_key =
            "copilot-cli\u{1f}session-other\u{1f}/tmp/events.jsonl\u{1f}0\u{1f}evt-1";
        let mut store = embed::EmbeddingStore::create(
            index_dir.path(),
            embed::DEFAULT_MODEL,
            embed::DEFAULT_DIM,
        );
        let mut live_keys = HashSet::new();
        let mut reused = 0;
        for (message_key, _, hash) in
            pending_embeddings(&session, &messages, &store, &mut live_keys, &mut reused)
        {
            store
                .upsert(&message_key, hash, vec![0.0; embed::DEFAULT_DIM as usize])
                .unwrap();
        }
        store
            .upsert(
                unrelated_key,
                embed::content_hash("other provider"),
                vec![0.0; embed::DEFAULT_DIM as usize],
            )
            .unwrap();
        store.flush().unwrap();

        let prune_providers = HashSet::from([Provider::ClaudeCode]);
        let summary = run_embeddings(
            index_dir.path(),
            &[session],
            &[],
            Some(&prune_providers),
            false,
        )
        .unwrap();
        let loaded = embed::EmbeddingStore::open(index_dir.path())
            .unwrap()
            .unwrap();

        assert_eq!(summary["errors"].as_array().unwrap().len(), 0);
        assert_eq!(summary["messages_pruned_from_store"], 0);
        assert!(
            loaded.get(unrelated_key).is_some(),
            "provider-scoped embedding run pruned an unrelated provider vector"
        );
    }
}
