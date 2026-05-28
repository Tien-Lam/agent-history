use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

mod discovery;
mod parse;
pub(crate) mod paths;

use super::{
    entry_is_regular_file, path_is_regular_file, HistoryProvider, ProviderError,
    ProviderFingerprintPath, ProviderMessageLoad, ProviderParseStats,
};
use crate::model::{Message, Provider, Session};
use discovery::{base_dirs, discover_sessions};
pub(crate) use parse::message_id_from_file;
use parse::parse_message_file_with_stats;

pub struct OpenCodeProvider {
    dirs: Vec<PathBuf>,
}

impl OpenCodeProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

impl HistoryProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        Provider::OpenCode
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        discover_sessions(&self.dirs)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        Ok(self.load_messages_with_stats(session)?.messages)
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        let storage_base = paths::storage_base_from_source_path(&session.source_path)
            .map_or_else(|| session.source_path.clone(), std::path::Path::to_path_buf);
        // Messages are in message/{sessionID}/msg_*.json. Keep accepting the
        // legacy storage-root source_path shape for callers with old Sessions.
        let message_dir = paths::message_dir(&storage_base, &session.id.0);
        let part_dir = paths::part_root(&storage_base);
        tracing::debug!(message_dir = %message_dir.display(), "loading OpenCode messages");
        if !message_dir.exists() {
            tracing::warn!(message_dir = %message_dir.display(), "message directory does not exist");
            return Ok(ProviderMessageLoad::from_messages(Vec::new()));
        }

        let mut messages = Vec::new();
        let mut parse_stats = ProviderParseStats::default();
        let files = std::fs::read_dir(&message_dir)?;

        for file_entry in files {
            let file_entry = file_entry?;
            if !entry_is_regular_file(&file_entry) {
                continue;
            }
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            if let Some(msg) = parse_message_file_with_stats(&path, &part_dir, &mut parse_stats) {
                messages.push(msg);
            }
        }

        messages.sort_by_key(|m| m.timestamp);
        tracing::info!(
            message_dir = %message_dir.display(),
            files = parse_stats.records_seen,
            parse_errors = parse_stats.parse_errors,
            skipped_records = parse_stats.skipped_records,
            empty_content = parse_stats.empty_content,
            messages = messages.len(),
            "OpenCode message loading complete"
        );
        Ok(ProviderMessageLoad {
            messages,
            parse_stats,
        })
    }

    fn index_fingerprint_paths(
        &self,
        session: &Session,
    ) -> io::Result<Vec<ProviderFingerprintPath>> {
        if !path_is_regular_file(&session.source_path) {
            return Ok(vec![ProviderFingerprintPath::new(
                "source",
                session.source_path.clone(),
            )]);
        }

        let Some(storage_base) = paths::storage_base_from_source_path(&session.source_path) else {
            return Ok(vec![ProviderFingerprintPath::new(
                "source",
                session.source_path.clone(),
            )]);
        };

        let mut fingerprint_paths = vec![ProviderFingerprintPath::new(
            "session",
            session.source_path.clone(),
        )];
        let message_dir = paths::message_dir(storage_base, &session.id.0);
        if message_dir.exists() {
            fingerprint_paths.push(ProviderFingerprintPath::new(
                "messages",
                message_dir.clone(),
            ));
            for (label, path) in opencode_part_dirs(&message_dir, storage_base)? {
                fingerprint_paths.push(ProviderFingerprintPath::new(label, path));
            }
        }
        Ok(fingerprint_paths)
    }
}

fn opencode_part_dirs(
    message_dir: &std::path::Path,
    storage_base: &std::path::Path,
) -> io::Result<BTreeMap<String, PathBuf>> {
    let part_root = paths::part_root(storage_base);
    if !part_root.exists() {
        return Ok(BTreeMap::new());
    }

    let mut dirs = BTreeMap::new();
    let entries = std::fs::read_dir(message_dir)?;
    for entry in entries {
        let entry = entry?;
        if !entry_is_regular_file(&entry) {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(message_id) = message_id_from_file(&path).or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        }) else {
            continue;
        };
        let part_dir = part_root.join(&message_id);
        if part_dir.exists() {
            dirs.insert(format!("parts/{message_id}"), part_dir);
        }
    }
    Ok(dirs)
}
