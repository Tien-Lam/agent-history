use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::super::args::{optional_usize, required_str};
use super::super::server::McpServer;

use crate::action::Action;
use crate::model::Session;
use crate::search::{HitKind, SearchIndex};

impl McpServer {
    pub(super) fn tool_search_sessions(&self, args: &Value) -> Result<Value, String> {
        let query = required_str(args, "query")?;
        if query.trim().is_empty() {
            return Err("query is empty".to_string());
        }
        let limit = optional_usize(args, "limit", 20, 1, 200)?;

        let sessions = self.collect_sessions();
        let index_dir = SearchIndex::default_index_dir();
        let index = SearchIndex::open_or_create(&index_dir)
            .map_err(|e| format!("failed to open search index: {e}"))?;

        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        index
            .build_index(&sessions, &self.providers, &tx)
            .map_err(|e| format!("failed to build index: {e}"))?;

        index_notes_best_effort(&index);

        let hits = index
            .search(&query, limit)
            .map_err(|e| format!("search failed: {e}"))?;

        let session_meta: HashMap<String, &Session> =
            sessions.iter().map(|s| (s.identity_key(), s)).collect();
        let turn_lookup = self.turn_lookup_for_hits(&hits, &session_meta);

        let mut hits_json = Vec::with_capacity(hits.len());
        for h in &hits {
            match h.kind {
                HitKind::Note => {
                    // Notes carry their own ref shape (`<provider>/<id>[#<turn>]`)
                    // so we surface it directly. Per-message fields do not apply.
                    hits_json.push(json!({
                        "kind": HitKind::Note.slug(),
                        "ref": h.note_session_ref,
                        "note_id": h.note_id,
                        "score": h.score,
                        "snippet": h.snippet,
                    }));
                }
                HitKind::Message => {
                    let session = session_meta.get(h.session_key.as_str()).copied();
                    let turn = turn_lookup.get(h.message_key.as_str()).copied();
                    let citation_ref = session
                        .zip(turn)
                        .and_then(|(s, t)| {
                            u32::try_from(t).ok().and_then(|turn| s.citation_ref(turn))
                        })
                        .map(|r| r.to_string());
                    hits_json.push(json!({
                        "kind": HitKind::Message.slug(),
                        "ref": citation_ref,
                        "session_id": h.session_id,
                        "message_id": h.message_id,
                        "turn": turn,
                        "score": h.score,
                        "snippet": h.snippet,
                        "provider": session.map(|s| s.provider.slug()),
                        "project": session.and_then(|s| s.project_name.as_deref()),
                        "started_at": session.map(|s| s.started_at),
                    }));
                }
            }
        }

        Ok(json!({
            "query": query,
            "limit": limit,
            "total": hits_json.len(),
            "hits": hits_json,
        }))
    }

    fn turn_lookup_for_hits(
        &self,
        hits: &[crate::search::SearchHit],
        session_meta: &HashMap<String, &Session>,
    ) -> HashMap<String, usize> {
        // Resolve message_id -> 1-based turn by loading messages once per
        // unique session that appears in the hit set. Without this the caller
        // cannot construct a citation ref from a search hit. Sessions that
        // cannot be loaded are silently dropped from the turn map; their hits
        // get `ref: null` and `turn: null`.
        let mut turn_lookup: HashMap<String, usize> = HashMap::new();
        let mut seen_sessions: HashSet<&str> = HashSet::new();
        for h in hits {
            if !seen_sessions.insert(h.session_key.as_str()) {
                continue;
            }
            let Some(session) = session_meta.get(h.session_key.as_str()).copied() else {
                continue;
            };
            let Some(provider) = self
                .providers
                .iter()
                .find(|p| p.provider() == session.provider)
            else {
                continue;
            };
            let Ok(messages) = provider.load_messages(session) else {
                continue;
            };
            for (i, m) in messages.iter().enumerate() {
                turn_lookup.insert(session.message_key(i, &m.id.0), i + 1);
            }
        }
        turn_lookup
    }
}

fn index_notes_best_effort(index: &SearchIndex) {
    // Best-effort: surface user notes alongside session messages. Sidecar
    // failures are intentionally swallowed. A missing metadata.db is the
    // common case and must not break MCP search.
    if let Some(path) = crate::metadata::default_path() {
        if path.exists() {
            if let Ok(conn) = crate::metadata::open(&path) {
                if let Ok(notes) = crate::metadata::note_list(&conn, None) {
                    let _ = index.index_notes(&notes);
                }
            }
        }
    }
}
