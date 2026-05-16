use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::action::Action;
use crate::embed;
use crate::model::Session;
use crate::search::SearchFilters;

use super::App;

/// Debounce window for live-updating search. Keystrokes within this window
/// coalesce into a single Tantivy query, run on the next event-loop tick.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(120);

/// RRF blend weight used by the TUI hybrid toggle. Mirrors the CLI's
/// "balanced" setting — lexical and semantic ranks contribute equally.
const HYBRID_WEIGHT: f32 = 0.5;

impl App {
    /// Whether the embedding pipeline is wired up for the current index.
    /// `false` on lean (no-feature) builds and on indexes that haven't been
    /// embedded yet. The TUI uses this to decide whether to show the hybrid
    /// indicator at all.
    pub fn hybrid_available(&self) -> bool {
        self.hybrid_available
    }

    /// User-visible state of the hybrid toggle. Independent of
    /// `hybrid_available` — when the toggle is on but the pipeline isn't
    /// ready, queries silently fall open to lexical-only and `last_engine()`
    /// reflects what actually ran.
    pub fn hybrid_enabled(&self) -> bool {
        self.hybrid_enabled
    }

    /// Engine that produced the current `search_results` (`"lexical"` or
    /// `"hybrid"`). Mirrors `meta.engine` from the CLI's JSON output.
    pub fn last_engine(&self) -> &'static str {
        self.last_engine
    }

    /// Test-only escape hatch that sidesteps the on-disk consent + embedding
    /// store probe. Exercises the ToggleHybrid path without standing up a
    /// real fastembed pipeline.
    #[doc(hidden)]
    pub fn set_hybrid_available_for_tests(&mut self, available: bool) {
        self.hybrid_available = available;
    }

    pub(super) fn start_indexing(&self) {
        let Some(index) = self.search_index.clone() else {
            return;
        };
        let sessions = self.sessions.clone();
        let providers = Arc::clone(&self.providers);
        let tx = self.action_tx.clone();

        std::thread::spawn(move || {
            match index.build_index_without_pruning(&sessions, &providers, &tx) {
                Ok(_) => {
                    let _ = tx.send(Action::IndexReady);
                }
                Err(e) => {
                    let _ = tx.send(Action::LoadError(format!("Index error: {e}")));
                }
            }
        });
    }

    /// Refresh `msg_filter_session_ids` from the search index when role or
    /// has-tool-call toggles change. If the index isn't ready yet, surface a
    /// status message and clear the cache so the filter is a no-op until the
    /// index finishes building (rather than silently hiding all sessions).
    pub(super) fn recompute_message_filter(&mut self) {
        if !self.filter.has_message_filter() {
            self.msg_filter_session_ids = None;
            return;
        }

        let Some(ref index) = self.search_index else {
            self.msg_filter_session_ids = None;
            self.status_message = Some("Filter unavailable — search index missing".to_string());
            return;
        };
        if !self.index_ready {
            self.msg_filter_session_ids = None;
            self.status_message =
                Some("Filter pending — wait for index to finish building".to_string());
            return;
        }

        match index.session_ids_with_messages(self.filter.role, self.filter.has_tool_call) {
            Ok(ids) => {
                self.msg_filter_session_ids = Some(ids);
            }
            Err(e) => {
                self.warnings.push(format!("Filter index error: {e}"));
                self.msg_filter_session_ids = None;
            }
        }
    }

    /// Run any pending debounced search if its idle window has elapsed.
    /// Called from the run loop and from tests.
    pub fn tick(&mut self) {
        if let Some(t) = self.search_pending_at {
            if t.elapsed() >= SEARCH_DEBOUNCE {
                self.execute_search();
                self.search_pending_at = None;
            }
        }
    }

    /// Whether a debounced search is queued but not yet executed.
    pub fn has_pending_search(&self) -> bool {
        self.search_pending_at.is_some()
    }

    pub(super) fn execute_search(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_session_ids = None;
            self.search_results.clear();
            self.last_engine = if self.hybrid_enabled && self.hybrid_available {
                "hybrid"
            } else {
                "lexical"
            };
            self.session_list.state.select(if self.sessions.is_empty() {
                None
            } else {
                Some(0)
            });
            return;
        }

        let Some(ref index) = self.search_index else {
            return;
        };
        if !self.index_ready {
            return;
        }

        // Try the hybrid pipeline first when the user has it on; fall open to
        // lexical-only on any failure (no consent, empty store, embedder
        // bootstrap fails, etc). `meta.engine` reflects what actually ran.
        let hybrid_hits = if self.hybrid_enabled && self.hybrid_available {
            embed::try_hybrid_search(
                &self.index_dir,
                index,
                &self.search_query,
                200,
                &SearchFilters::default(),
                HYBRID_WEIGHT,
            )
        } else {
            None
        };

        let (engine, hits_result) = if let Some(hits) = hybrid_hits {
            ("hybrid", Ok(hits))
        } else {
            ("lexical", index.search(&self.search_query, 200))
        };

        if let Ok(hits) = hits_result {
            self.last_engine = engine;
            let current_session_keys: HashSet<String> =
                self.sessions.iter().map(Session::identity_key).collect();
            let hits: Vec<_> = hits
                .into_iter()
                .filter(|hit| current_session_keys.contains(&hit.session_key))
                .collect();
            let mut seen = HashSet::new();
            let ids: Vec<String> = hits
                .iter()
                .filter(|h| seen.insert(h.session_key.clone()))
                .map(|h| h.session_key.clone())
                .collect();
            self.filtered_session_ids = Some(ids);
            self.search_results = hits;
            let count = self.display_count();
            self.session_list
                .state
                .select(if count > 0 { Some(0) } else { None });
        } else {
            self.filtered_session_ids = None;
            self.search_results.clear();
        }
    }
}
