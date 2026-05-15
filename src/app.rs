use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Debounce window for live-updating search. Keystrokes within this window
/// coalesce into a single Tantivy query, run on the next event-loop tick.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(120);

/// RRF blend weight used by the TUI hybrid toggle. Mirrors the CLI's
/// "balanced" setting — lexical and semantic ranks contribute equally.
const HYBRID_WEIGHT: f32 = 0.5;

use chrono::Utc;
use crossterm::event::Event;
use lru::LruCache;
use ratatui::prelude::Backend;
use ratatui::Terminal;

use crate::action::Action;
use crate::config::Config;
use crate::embed;
use crate::event::{map_key_event, CrosstermEventSource, EventSource};
use crate::export::ExportFormat;
use crate::model::{Message, Provider, Session, SessionId};
use crate::provider::HistoryProvider;
use crate::search::{SearchFilters, SearchHit, SearchIndex};
use crate::stars::StarStore;
use crate::ui::message_view::MessageViewComponent;
use crate::ui::session_list::SessionListComponent;
use crate::ui::status_bar::StatusBarComponent;

mod overlays;
mod render;
mod state;
use overlays::push_date_char;
pub use state::AppMode;
use state::{cycle_role, FilterField, FilterState};

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

    fn start_indexing(&self) {
        let Some(index) = self.search_index.clone() else {
            return;
        };
        let sessions = self.sessions.clone();
        let providers = Arc::clone(&self.providers);
        let tx = self.action_tx.clone();

        std::thread::spawn(
            move || match index.build_index(&sessions, &providers, &tx) {
                Ok(_) => {
                    let _ = tx.send(Action::IndexReady);
                }
                Err(e) => {
                    let _ = tx.send(Action::LoadError(format!("Index error: {e}")));
                }
            },
        );
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

    /// Refresh `msg_filter_session_ids` from the search index when role or
    /// has-tool-call toggles change. If the index isn't ready yet, surface a
    /// status message and clear the cache so the filter is a no-op until the
    /// index finishes building (rather than silently hiding all sessions).
    fn recompute_message_filter(&mut self) {
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

    fn execute_search(&mut self) {
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

    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }

            Action::NextItem
            | Action::PrevItem
            | Action::SelectSession
            | Action::SearchSubmit
            | Action::BackToList
            | Action::GoToTop
            | Action::GoToBottom
            | Action::ScrollUp
            | Action::ScrollDown
            | Action::PageUp
            | Action::PageDown
            | Action::ToggleToolCalls
            | Action::ToggleRawToolOutput
            | Action::ToggleHelp => self.dispatch_navigation(&action),

            Action::SearchStart
            | Action::SearchInput(_)
            | Action::SearchBackspace
            | Action::ToggleHybrid
            | Action::SearchCancel => self.dispatch_search(&action),

            Action::IndexProgress(_, _) | Action::IndexReady => self.dispatch_index(&action),

            Action::ToggleFilter
            | Action::FilterNext
            | Action::FilterPrev
            | Action::FilterToggle
            | Action::FilterEdit
            | Action::FilterEditDone
            | Action::FilterInput(_)
            | Action::FilterBackspace
            | Action::FilterClearAll => self.dispatch_filter(&action),

            // Stars / bookmarks
            Action::ToggleStar => {
                if let Some((session_id, _, provider)) = self.resolve_selected_session() {
                    match self.stars.toggle(provider, &session_id) {
                        Ok(true) => {
                            self.status_message = Some("Starred".to_string());
                        }
                        Ok(false) => {
                            self.status_message = Some("Unstarred".to_string());
                            // If we just unstarred while filtering by starred-only,
                            // the selection may now point past the end of the list.
                            self.clamp_selection();
                        }
                        Err(e) => {
                            self.warnings.push(format!("Failed to save stars: {e}"));
                        }
                    }
                }
            }

            // Resume
            Action::CopyResumeCommand => {
                if let Some((session_id, _, provider)) = self.resolve_selected_session() {
                    let cmd = provider.resume_command(&session_id);
                    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&cmd)) {
                        Ok(()) => {
                            self.status_message = Some(format!("Copied: {cmd}"));
                        }
                        Err(_) => {
                            self.status_message = Some(format!("Resume: {cmd}"));
                        }
                    }
                }
            }

            Action::ExportStart
            | Action::ExportNext
            | Action::ExportPrev
            | Action::ExportConfirm
            | Action::ExportCancel => self.dispatch_export(&action),

            Action::SessionsLoaded(_) | Action::MessagesLoaded(_, _) | Action::LoadError(_) => {
                self.dispatch_data(action);
            }

            Action::Resize(_, _) | Action::SwitchFocus => {}
        }
    }

    fn dispatch_navigation(&mut self, action: &Action) {
        match action {
            Action::NextItem => {
                if let Some(selected) = self.session_list.selected_index() {
                    let count = self.display_count();
                    if selected + 1 < count {
                        self.session_list.state.select(Some(selected + 1));
                        self.preload_focused_session();
                    }
                }
            }
            Action::PrevItem => {
                if let Some(selected) = self.session_list.selected_index() {
                    if selected > 0 {
                        self.session_list.state.select(Some(selected - 1));
                        self.preload_focused_session();
                    }
                }
            }
            Action::SelectSession | Action::SearchSubmit => {
                if self.search_pending_at.take().is_some() {
                    self.execute_search();
                }
                if let Some((session_id, source_path, provider)) = self.resolve_selected_session() {
                    self.load_messages_cached(&session_id, &source_path, provider);
                    self.message_view.reset_scroll();
                    self.mode = AppMode::ViewSession;
                }
            }
            Action::BackToList => {
                self.mode = AppMode::Browse;
            }
            Action::GoToTop => match self.mode {
                AppMode::Browse | AppMode::Search => {
                    let count = self.display_count();
                    if count > 0 {
                        self.session_list.state.select(Some(0));
                        self.preload_focused_session();
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = 0;
                }
                AppMode::Help | AppMode::Filter | AppMode::ExportMenu => {}
            },
            Action::GoToBottom => match self.mode {
                AppMode::Browse | AppMode::Search => {
                    let count = self.display_count();
                    if count > 0 {
                        self.session_list.state.select(Some(count - 1));
                        self.preload_focused_session();
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = u16::MAX;
                }
                AppMode::Help | AppMode::Filter | AppMode::ExportMenu => {}
            },
            Action::ScrollUp => {
                self.message_view.scroll_up(1);
            }
            Action::ScrollDown => {
                self.message_view.scroll_down(1);
            }
            Action::PageUp => {
                self.message_view.scroll_up(20);
            }
            Action::PageDown => {
                self.message_view.scroll_down(20);
            }
            Action::ToggleToolCalls => {
                self.message_view.show_tool_calls = !self.message_view.show_tool_calls;
            }
            Action::ToggleRawToolOutput => {
                self.message_view.show_raw_output = !self.message_view.show_raw_output;
                // Auto-expand tool calls when entering raw mode — a "raw"
                // toggle that produces no visible output would just confuse.
                if self.message_view.show_raw_output {
                    self.message_view.show_tool_calls = true;
                }
                self.status_message = Some(if self.message_view.show_raw_output {
                    "Raw tool output: ON".to_string()
                } else {
                    "Raw tool output: OFF".to_string()
                });
            }
            Action::ToggleHelp => {
                if self.mode == AppMode::Help {
                    self.mode = self.pre_help_mode;
                } else {
                    self.pre_help_mode = self.mode;
                    self.mode = AppMode::Help;
                }
            }
            _ => {}
        }
    }

    fn dispatch_search(&mut self, action: &Action) {
        match action {
            Action::SearchStart => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.search_pending_at = None;
                self.mode = AppMode::Search;
            }
            Action::SearchInput(c) => {
                self.search_query.push(*c);
                self.search_pending_at = Some(Instant::now());
            }
            Action::SearchBackspace => {
                self.search_query.pop();
                self.search_pending_at = Some(Instant::now());
            }
            Action::ToggleHybrid => {
                if self.hybrid_available {
                    self.hybrid_enabled = !self.hybrid_enabled;
                    self.status_message = Some(if self.hybrid_enabled {
                        "Hybrid search: ON".to_string()
                    } else {
                        "Hybrid search: OFF".to_string()
                    });
                    // Re-run the current query so the engine label and
                    // result ordering reflect the new mode immediately.
                    if self.search_query.is_empty() {
                        self.last_engine = if self.hybrid_enabled {
                            "hybrid"
                        } else {
                            "lexical"
                        };
                    } else {
                        self.search_pending_at = None;
                        self.execute_search();
                    }
                } else {
                    self.status_message = Some(
                        "Hybrid search unavailable — run `aghist index --accept-download`"
                            .to_string(),
                    );
                }
            }
            Action::SearchCancel => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.search_pending_at = None;
                self.session_list.state.select(if self.sessions.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.mode = AppMode::Browse;
            }
            _ => {}
        }
    }

    fn dispatch_index(&mut self, action: &Action) {
        match action {
            Action::IndexProgress(done, total) => {
                self.index_progress = Some((*done, *total));
            }
            Action::IndexReady => {
                self.index_ready = true;
                self.index_progress = None;
                // If a message-level filter was set while the index was still
                // building, resolve it now that we have data.
                if self.filter.has_message_filter() && self.msg_filter_session_ids.is_none() {
                    self.recompute_message_filter();
                }
            }
            _ => {}
        }
    }

    fn dispatch_filter(&mut self, action: &Action) {
        match action {
            Action::ToggleFilter => {
                if self.mode == AppMode::Filter {
                    self.apply_filters();
                    self.mode = AppMode::Browse;
                } else {
                    self.mode = AppMode::Filter;
                }
            }
            Action::FilterNext => {
                let max = FilterState::item_count();
                if self.filter.cursor + 1 < max {
                    self.filter.editing_field = None;
                    self.filter.cursor += 1;
                }
            }
            Action::FilterPrev if self.filter.cursor > 0 => {
                self.filter.editing_field = None;
                self.filter.cursor -= 1;
            }
            Action::FilterToggle => {
                let providers = Provider::all();
                if self.filter.cursor < providers.len() {
                    let p = providers[self.filter.cursor];
                    let enabled = self.filter.provider_enabled.entry(p).or_insert(true);
                    *enabled = !*enabled;
                } else if self.filter.cursor == FilterState::role_idx() {
                    self.filter.role = cycle_role(self.filter.role);
                    self.recompute_message_filter();
                } else if self.filter.cursor == FilterState::tool_call_idx() {
                    self.filter.has_tool_call = !self.filter.has_tool_call;
                    self.recompute_message_filter();
                } else if self.filter.cursor == FilterState::starred_idx() {
                    self.filter.starred_only = !self.filter.starred_only;
                }
            }
            Action::FilterEdit => {
                let providers = Provider::all();
                let offset = self.filter.cursor.saturating_sub(providers.len());
                self.filter.editing_field = match offset {
                    0 => Some(FilterField::Project),
                    1 => Some(FilterField::DateFrom),
                    2 => Some(FilterField::DateTo),
                    _ => None,
                };
            }
            Action::FilterEditDone => {
                self.filter.editing_field = None;
            }
            Action::FilterInput(c) => match self.filter.editing_field {
                Some(FilterField::Project) => self.filter.project_query.push(*c),
                Some(FilterField::DateFrom) => {
                    push_date_char(&mut self.filter.date_from, *c);
                }
                Some(FilterField::DateTo) => {
                    push_date_char(&mut self.filter.date_to, *c);
                }
                None => {
                    // Space toggles provider checkboxes.
                    if *c == ' ' {
                        self.dispatch(Action::FilterToggle);
                    }
                }
            },
            Action::FilterBackspace => match self.filter.editing_field {
                Some(FilterField::Project) => {
                    self.filter.project_query.pop();
                }
                Some(FilterField::DateFrom) => {
                    self.filter.date_from = None;
                }
                Some(FilterField::DateTo) => {
                    self.filter.date_to = None;
                }
                None => {}
            },
            Action::FilterClearAll => {
                self.filter = FilterState::new();
                self.msg_filter_session_ids = None;
            }
            _ => {}
        }
    }

    fn dispatch_export(&mut self, action: &Action) {
        match action {
            Action::ExportStart => {
                self.export_cursor = 0;
                self.status_message = None;
                self.mode = AppMode::ExportMenu;
            }
            Action::ExportNext if self.export_cursor + 1 < ExportFormat::all().len() => {
                self.export_cursor += 1;
            }
            Action::ExportPrev if self.export_cursor > 0 => {
                self.export_cursor -= 1;
            }
            Action::ExportConfirm => {
                let format = ExportFormat::all()[self.export_cursor];
                self.perform_export(format);
                self.mode = AppMode::ViewSession;
            }
            Action::ExportCancel => {
                self.mode = AppMode::ViewSession;
            }
            _ => {}
        }
    }

    fn dispatch_data(&mut self, action: Action) {
        match action {
            Action::SessionsLoaded(sessions) => {
                self.sessions = sessions;
                self.loading = false;
                self.search_results.clear();
                self.filtered_session_ids = None;
                if !self.sessions.is_empty() {
                    self.session_list.state.select(Some(0));
                    self.preload_focused_session();
                }
            }
            Action::MessagesLoaded(session_id, messages) => {
                self.message_cache.put(session_id.0, messages);
            }
            Action::LoadError(msg) => {
                self.warnings.push(msg);
            }
            _ => {}
        }
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

    fn perform_export(&mut self, format: ExportFormat) {
        let session = {
            let Some(idx) = self.session_list.selected_index() else {
                return;
            };
            let display = self.display_sessions();
            match display.get(idx) {
                Some(s) => (*s).clone(),
                None => return,
            }
        };

        let messages = match self.message_cache.get(&session.id.0) {
            Some(m) => m.clone(),
            None => return,
        };

        let content = crate::export::export(format, &session, &messages);
        let id_short = session.id.0.get(..8).unwrap_or(&session.id.0);
        let sanitized: String = id_short
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let filename = format!("aghist-{sanitized}.{}", format.extension());

        match std::fs::write(&filename, &content) {
            Ok(()) => {
                self.status_message = Some(format!("Exported to {filename}"));
            }
            Err(e) => {
                self.warnings.push(format!("Export failed: {e}"));
            }
        }
    }
}
