use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::Event;
use lru::LruCache;
use ratatui::prelude::Backend;
use ratatui::Terminal;

use crate::action::Action;
use crate::config::Config;
use crate::embed;
use crate::event::{map_key_event, CrosstermEventSource, EventSource};
use crate::model::{Message, Provider, Session, SessionId};
use crate::provider::HistoryProvider;
use crate::search::{SearchHit, SearchIndex};
use crate::stars::StarStore;
use crate::ui::message_view::MessageViewComponent;
use crate::ui::session_list::SessionListComponent;
use crate::ui::status_bar::StatusBarComponent;

mod dispatch;
mod export;
mod overlays;
mod render;
mod search;
mod state;
pub use state::AppMode;
use state::FilterState;

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    config: Config,
    sessions: Vec<Session>,
    message_cache: LruCache<String, Vec<Message>>,
    mode: AppMode,
    loading: bool,
    should_quit: bool,
    warnings: Vec<String>,

    session_list: SessionListComponent,
    message_view: MessageViewComponent,
    status_bar: StatusBarComponent,

    providers: Arc<Vec<Box<dyn HistoryProvider>>>,
    action_rx: crossbeam_channel::Receiver<Action>,
    action_tx: crossbeam_channel::Sender<Action>,

    search_index: Option<Arc<SearchIndex>>,
    index_dir: std::path::PathBuf,
    search_query: String,
    search_results: Vec<SearchHit>,
    filtered_session_ids: Option<Vec<String>>,
    index_ready: bool,
    index_progress: Option<(usize, usize)>,
    search_pending_at: Option<Instant>,
    /// True when a hybrid search pipeline is wired up for this index dir
    /// (feature compiled in + consent recorded + non-empty store). Decided
    /// at startup; toggled to `false` if a later check fails.
    hybrid_available: bool,
    /// User-controlled toggle: when `true` and `hybrid_available`, queries
    /// run through RRF. Defaults on when available.
    hybrid_enabled: bool,
    /// Engine that produced `search_results` (`"lexical"` or `"hybrid"`).
    /// Surfaced in the status bar so users can see whether their toggle
    /// actually engaged the semantic side.
    last_engine: &'static str,

    filter: FilterState,
    /// Session IDs returned by the message-level filter (role / has-tool-call)
    /// resolved against the search index. `None` means no message-level filter
    /// is active; an empty set means the filter is active but matched nothing.
    msg_filter_session_ids: Option<HashSet<String>>,
    export_cursor: usize,
    pre_help_mode: AppMode,
    pub status_message: Option<String>,
    stars: StarStore,
}

impl App {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>, config: Config) -> Self {
        Self::with_stars(providers, config, StarStore::load_default())
    }

    /// Construct an `App` with an explicit `StarStore`. Tests use this to
    /// avoid touching the user's real metadata sidecar.
    pub fn with_stars(
        providers: Vec<Box<dyn HistoryProvider>>,
        config: Config,
        stars: StarStore,
    ) -> Self {
        let (action_tx, action_rx) = crossbeam_channel::unbounded();
        let cache_size = NonZeroUsize::new(config.cache_size).unwrap_or(NonZeroUsize::MIN);

        let mut message_view = MessageViewComponent::new();
        message_view.show_tool_calls = config.show_tool_calls;

        Self {
            config,
            sessions: Vec::new(),
            message_cache: LruCache::new(cache_size),
            mode: AppMode::Browse,
            loading: true,
            should_quit: false,
            warnings: Vec::new(),

            session_list: SessionListComponent::new(),
            message_view,
            status_bar: StatusBarComponent::new(),

            providers: Arc::new(providers),
            action_rx,
            action_tx,

            search_index: None,
            index_dir: SearchIndex::default_index_dir(),
            search_query: String::new(),
            search_results: Vec::new(),
            filtered_session_ids: None,
            index_ready: false,
            index_progress: None,
            search_pending_at: None,
            hybrid_available: false,
            hybrid_enabled: false,
            last_engine: "lexical",

            filter: FilterState::new(),
            msg_filter_session_ids: None,
            export_cursor: 0,
            pre_help_mode: AppMode::Browse,
            status_message: None,
            stars,
        }
    }

    pub fn mode(&self) -> AppMode {
        self.mode
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.session_list.selected_index()
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn run<B: Backend<Error: Send + Sync + 'static>>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> anyhow::Result<()> {
        self.run_with_event_source(terminal, CrosstermEventSource)
    }

    pub fn run_with_event_source<B: Backend<Error: Send + Sync + 'static>>(
        &mut self,
        terminal: &mut Terminal<B>,
        mut events: impl EventSource,
    ) -> anyhow::Result<()> {
        self.index_dir = SearchIndex::default_index_dir();
        self.search_index = SearchIndex::open_or_create(&self.index_dir)
            .map(Arc::new)
            .ok();
        self.hybrid_available = embed::hybrid_ready(&self.index_dir);
        // Default ON when the embedding pipeline is wired up — hybrid is
        // strictly an improvement over lexical when the store is populated.
        // Users can still hit the toggle to compare modes side-by-side.
        self.hybrid_enabled = self.hybrid_available;

        self.load_sessions();
        self.start_indexing();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(Event::Key(key)) = events.poll_event(Duration::from_millis(50))? {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }
                let editing = self.filter.editing_field.is_some();
                if let Some(action) = map_key_event(key, self.mode, editing) {
                    self.dispatch(action);
                }
            }

            while let Ok(action) = self.action_rx.try_recv() {
                self.dispatch(action);
            }

            self.tick();

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    pub fn load_sessions(&mut self) {
        let tx = self.action_tx.clone();
        let mut all_sessions = Vec::new();

        for provider in &*self.providers {
            match provider.discover_sessions() {
                Ok(sessions) => all_sessions.extend(sessions),
                Err(e) => {
                    let _ = tx.send(Action::LoadError(format!("{}: {e}", provider.provider())));
                }
            }
        }

        all_sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        let _ = tx.send(Action::SessionsLoaded(all_sessions));

        while let Ok(action) = self.action_rx.try_recv() {
            self.dispatch(action);
        }
    }

    fn display_sessions(&self) -> Vec<&Session> {
        let base: Vec<&Session> = if let Some(ref ids) = self.filtered_session_ids {
            ids.iter()
                .filter_map(|id| self.sessions.iter().find(|s| s.identity_key() == *id))
                .collect()
        } else {
            self.sessions.iter().collect()
        };

        let starred_only = self.filter.starred_only;
        let msg_ids = self.msg_filter_session_ids.as_ref();
        if self.filter.is_active() {
            base.into_iter()
                .filter(|s| self.filter.matches(s))
                .filter(|s| !starred_only || self.stars.is_starred(s.provider, &s.id.0))
                .filter(|s| msg_ids.is_none_or(|ids| ids.contains(&s.identity_key())))
                .collect()
        } else {
            base
        }
    }

    fn display_count(&self) -> usize {
        self.display_sessions().len()
    }

    fn resolve_selected_session(&self) -> Option<(String, std::path::PathBuf, Provider)> {
        let idx = self.session_list.selected_index()?;
        let display = self.display_sessions();
        display
            .get(idx)
            .map(|s| (s.id.0.clone(), s.source_path.clone(), s.provider))
    }

    /// Ensure the selection index sits within the displayed-session range.
    /// Called after operations that may shrink the visible list (e.g.
    /// unstarring while the starred-only filter is active).
    fn clamp_selection(&mut self) {
        let count = self.display_count();
        let new_sel = match self.session_list.selected_index() {
            _ if count == 0 => None,
            Some(i) if i >= count => Some(count - 1),
            Some(i) => Some(i),
            None => Some(0),
        };
        self.session_list.state.select(new_sel);
        self.preload_focused_session();
    }

    fn apply_filters(&mut self) {
        let count = self.display_count();
        self.session_list
            .state
            .select(if count > 0 { Some(0) } else { None });
        self.preload_focused_session();
    }

    fn load_messages_cached(
        &mut self,
        session_id: &str,
        source_path: &std::path::Path,
        provider_type: Provider,
    ) {
        if self.message_cache.contains(session_id) {
            tracing::debug!(session_id, "message cache hit");
            return;
        }

        tracing::debug!(
            session_id,
            source_path = %source_path.display(),
            provider = ?provider_type,
            "loading messages (cache miss)"
        );

        let tmp_session = Session {
            id: SessionId(session_id.to_string()),
            provider: provider_type,
            project_path: None,
            project_name: None,
            git_branch: None,
            started_at: Utc::now(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 0,
            source_path: source_path.to_path_buf(),
        };

        let provider = self
            .providers
            .iter()
            .find(|p| p.provider() == provider_type);

        if let Some(provider) = provider {
            match provider.load_messages(&tmp_session) {
                Ok(mut messages) => {
                    tracing::info!(
                        session_id,
                        provider = ?provider_type,
                        message_count = messages.len(),
                        "messages loaded successfully"
                    );
                    let max = self.config.max_messages_per_session;
                    if messages.len() > max {
                        let total = messages.len();
                        messages.truncate(max);
                        self.warnings.push(format!(
                            "Session truncated: showing {max} of {total} messages"
                        ));
                    }
                    if messages.is_empty() {
                        tracing::warn!(
                            session_id,
                            source_path = %source_path.display(),
                            "provider returned 0 messages — possible format mismatch"
                        );
                    }
                    self.message_cache.put(session_id.to_string(), messages);
                }
                Err(e) => {
                    tracing::error!(
                        session_id,
                        source_path = %source_path.display(),
                        error = %e,
                        "failed to load messages"
                    );
                    let _ = self
                        .action_tx
                        .send(Action::LoadError(format!("Failed to load messages: {e}")));
                }
            }
        } else {
            tracing::error!(
                session_id,
                provider = ?provider_type,
                "no matching provider found for session"
            );
        }
    }

    /// Load messages for whichever session is currently focused in the list,
    /// so the message panel always shows content alongside the session list.
    fn preload_focused_session(&mut self) {
        if let Some((session_id, source_path, provider)) = self.resolve_selected_session() {
            self.load_messages_cached(&session_id, &source_path, provider);
            self.message_view.reset_scroll();
        }
    }
}
