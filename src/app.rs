use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use lru::LruCache;

use crate::action::Action;
use crate::config::Config;
use crate::model::{Message, Session};
use crate::provider::HistoryProvider;
use crate::search::{SearchHit, SearchIndex};
use crate::stars::StarStore;
use crate::ui::message_view::MessageViewComponent;
use crate::ui::session_list::SessionListComponent;
use crate::ui::status_bar::StatusBarComponent;

mod dispatch;
mod export;
mod loading;
mod messages;
mod overlays;
mod render;
mod runtime;
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
}
