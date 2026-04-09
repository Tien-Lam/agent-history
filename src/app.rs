use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use crossterm::event::Event;
use lru::LruCache;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Backend;
use ratatui::Terminal;

use crate::action::Action;
use crate::config::Config;
use crate::event::{map_key_event, poll_event};
use crate::model::{Message, Provider, Session, SessionId};
use crate::provider::HistoryProvider;
use crate::search::{SearchHit, SearchIndex};
use crate::ui::message_view::MessageViewComponent;
use crate::ui::session_list::SessionListComponent;
use crate::ui::status_bar::StatusBarComponent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browse,
    ViewSession,
    Search,
    Help,
    Filter,
}

#[derive(Debug, Clone)]
pub struct FilterState {
    pub provider_enabled: std::collections::HashMap<Provider, bool>,
    pub project_query: String,
    pub date_from: Option<chrono::NaiveDate>,
    pub date_to: Option<chrono::NaiveDate>,
    pub cursor: usize,
    pub editing_field: Option<FilterField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Project,
    DateFrom,
    DateTo,
}

impl FilterState {
    fn new() -> Self {
        let mut provider_enabled = std::collections::HashMap::new();
        for p in Provider::all() {
            provider_enabled.insert(*p, true);
        }
        Self {
            provider_enabled,
            project_query: String::new(),
            date_from: None,
            date_to: None,
            cursor: 0,
            editing_field: None,
        }
    }

    fn is_active(&self) -> bool {
        self.provider_enabled.values().any(|v| !v)
            || !self.project_query.is_empty()
            || self.date_from.is_some()
            || self.date_to.is_some()
    }

    fn matches(&self, session: &Session) -> bool {
        if !self.provider_enabled.get(&session.provider).copied().unwrap_or(true) {
            return false;
        }

        if !self.project_query.is_empty() {
            let query = self.project_query.to_lowercase();
            let name_match = session
                .project_name
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(&query));
            let path_match = session
                .project_path
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|p| p.to_lowercase().contains(&query));
            if !name_match && !path_match {
                return false;
            }
        }

        if let Some(from) = self.date_from {
            if session.started_at.date_naive() < from {
                return false;
            }
        }
        if let Some(to) = self.date_to {
            if session.started_at.date_naive() > to {
                return false;
            }
        }

        true
    }

    fn item_count() -> usize {
        Provider::all().len() + 3 // providers + project + date_from + date_to
    }
}

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
    search_query: String,
    search_results: Vec<SearchHit>,
    filtered_session_ids: Option<Vec<String>>,
    index_ready: bool,
    index_progress: Option<(usize, usize)>,

    filter: FilterState,
}

impl App {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>, config: Config) -> Self {
        let (action_tx, action_rx) = crossbeam_channel::unbounded();
        let cache_size = NonZeroUsize::new(config.cache_size).unwrap_or(NonZeroUsize::new(20).unwrap());

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
            search_query: String::new(),
            search_results: Vec::new(),
            filtered_session_ids: None,
            index_ready: false,
            index_progress: None,

            filter: FilterState::new(),
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

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        self.search_index = SearchIndex::open_or_create(&SearchIndex::default_index_dir())
            .map(Arc::new)
            .ok();

        self.load_sessions();
        self.start_indexing();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(Event::Key(key)) = poll_event(Duration::from_millis(50))? {
                if let Some(action) = map_key_event(key, self.mode) {
                    self.dispatch(action);
                }
            }

            while let Ok(action) = self.action_rx.try_recv() {
                self.dispatch(action);
            }

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

        all_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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

        std::thread::spawn(move || {
            match index.build_index(&sessions, &providers, &tx) {
                Ok(_) => {
                    let _ = tx.send(Action::IndexReady);
                }
                Err(e) => {
                    let _ = tx.send(Action::LoadError(format!("Index error: {e}")));
                }
            }
        });
    }

    fn display_sessions(&self) -> Vec<&Session> {
        let base: Vec<&Session> = if let Some(ref ids) = self.filtered_session_ids {
            ids.iter()
                .filter_map(|id| self.sessions.iter().find(|s| s.id.0 == *id))
                .collect()
        } else {
            self.sessions.iter().collect()
        };

        if self.filter.is_active() {
            base.into_iter().filter(|s| self.filter.matches(s)).collect()
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
        display.get(idx).map(|s| (s.id.0.clone(), s.source_path.clone(), s.provider))
    }

    fn apply_filters(&mut self) {
        let count = self.display_count();
        self.session_list
            .state
            .select(if count > 0 { Some(0) } else { None });
    }

    fn execute_search(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_session_ids = None;
            self.search_results.clear();
            self.session_list.state.select(if self.sessions.is_empty() { None } else { Some(0) });
            return;
        }

        let Some(ref index) = self.search_index else {
            return;
        };
        if !self.index_ready {
            return;
        }

        if let Ok(hits) = index.search(&self.search_query, 200) {
            let mut seen = HashSet::new();
            let ids: Vec<String> = hits
                .iter()
                .filter(|h| seen.insert(h.session_id.clone()))
                .map(|h| h.session_id.clone())
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

    #[allow(clippy::too_many_lines)]
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }

            Action::NextItem => {
                if let Some(selected) = self.session_list.selected_index() {
                    let count = self.display_count();
                    if selected + 1 < count {
                        self.session_list.state.select(Some(selected + 1));
                    }
                }
            }
            Action::PrevItem => {
                if let Some(selected) = self.session_list.selected_index() {
                    if selected > 0 {
                        self.session_list.state.select(Some(selected - 1));
                    }
                }
            }
            Action::SelectSession | Action::SearchSubmit => {
                if let Some((session_id, source_path, provider)) =
                    self.resolve_selected_session()
                {
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
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = 0;
                }
                AppMode::Help | AppMode::Filter => {}
            },
            Action::GoToBottom => match self.mode {
                AppMode::Browse | AppMode::Search => {
                    let count = self.display_count();
                    if count > 0 {
                        self.session_list.state.select(Some(count - 1));
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = u16::MAX;
                }
                AppMode::Help | AppMode::Filter => {}
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

            Action::ToggleHelp => {
                self.mode = if self.mode == AppMode::Help {
                    AppMode::Browse
                } else {
                    AppMode::Help
                };
            }

            // Search
            Action::SearchStart => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.mode = AppMode::Search;
            }
            Action::SearchInput(c) => {
                self.search_query.push(c);
                self.execute_search();
            }
            Action::SearchBackspace => {
                self.search_query.pop();
                self.execute_search();
            }
            Action::SearchCancel => {
                self.search_query.clear();
                self.search_results.clear();
                self.filtered_session_ids = None;
                self.session_list
                    .state
                    .select(if self.sessions.is_empty() { None } else { Some(0) });
                self.mode = AppMode::Browse;
            }

            // Index
            Action::IndexProgress(done, total) => {
                self.index_progress = Some((done, total));
            }
            Action::IndexReady => {
                self.index_ready = true;
                self.index_progress = None;
            }

            // Filter
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
            Action::FilterPrev => {
                if self.filter.cursor > 0 {
                    self.filter.editing_field = None;
                    self.filter.cursor -= 1;
                }
            }
            Action::FilterToggle => {
                let providers = Provider::all();
                if self.filter.cursor < providers.len() {
                    let p = providers[self.filter.cursor];
                    let enabled = self.filter.provider_enabled.entry(p).or_insert(true);
                    *enabled = !*enabled;
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
            Action::FilterInput(c) => {
                match self.filter.editing_field {
                    Some(FilterField::Project) => self.filter.project_query.push(c),
                    Some(FilterField::DateFrom) => {
                        push_date_char(&mut self.filter.date_from, c);
                    }
                    Some(FilterField::DateTo) => {
                        push_date_char(&mut self.filter.date_to, c);
                    }
                    None => {
                        // Space toggles provider checkboxes
                        if c == ' ' {
                            self.dispatch(Action::FilterToggle);
                        }
                    }
                }
            }
            Action::FilterBackspace => {
                match self.filter.editing_field {
                    Some(FilterField::Project) => { self.filter.project_query.pop(); }
                    Some(FilterField::DateFrom) => { self.filter.date_from = None; }
                    Some(FilterField::DateTo) => { self.filter.date_to = None; }
                    None => {}
                }
            }
            Action::FilterClearAll => {
                self.filter = FilterState::new();
            }

            // Data events
            Action::SessionsLoaded(sessions) => {
                self.sessions = sessions;
                self.loading = false;
                if !self.sessions.is_empty() {
                    self.session_list.state.select(Some(0));
                }
            }
            Action::MessagesLoaded(session_id, messages) => {
                self.message_cache.put(session_id.0, messages);
            }
            Action::LoadError(msg) => {
                self.warnings.push(msg);
            }

            Action::Resize(_, _) | Action::SwitchFocus => {}
        }
    }

    fn load_messages_cached(
        &mut self,
        session_id: &str,
        source_path: &std::path::Path,
        provider_type: Provider,
    ) {
        if self.message_cache.contains(session_id) {
            return;
        }

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

        let provider = self.providers.iter().find(|p| p.provider() == provider_type);

        if let Some(provider) = provider {
            match provider.load_messages(&tmp_session) {
                Ok(mut messages) => {
                    let max = self.config.max_messages_per_session;
                    if messages.len() > max {
                        let total = messages.len();
                        messages.truncate(max);
                        self.warnings.push(format!(
                            "Session truncated: showing {max} of {total} messages"
                        ));
                    }
                    self.message_cache.put(session_id.to_string(), messages);
                }
                Err(e) => {
                    let _ = self
                        .action_tx
                        .send(Action::LoadError(format!("Failed to load messages: {e}")));
                }
            }
        }
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // main content
                Constraint::Length(1), // status bar
            ])
            .split(size);

        let content_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // session list
                Constraint::Percentage(70), // conversation detail
            ])
            .split(main_layout[0]);

        let base: Vec<&Session> = if let Some(ref ids) = self.filtered_session_ids {
            ids.iter()
                .filter_map(|id| self.sessions.iter().find(|s| s.id.0 == *id))
                .collect()
        } else {
            self.sessions.iter().collect()
        };
        let display: Vec<&Session> = if self.filter.is_active() {
            base.into_iter().filter(|s| self.filter.matches(s)).collect()
        } else {
            base
        };

        // Session list
        let list_focused = self.mode == AppMode::Browse || self.mode == AppMode::Search;
        self.session_list
            .render(&display, list_focused, frame, content_layout[0]);

        // Message view
        let selected_idx = self.session_list.selected_index();
        let selected_session = selected_idx.and_then(|i| display.get(i).copied());
        let messages = selected_session
            .and_then(|s| self.message_cache.get(&s.id.0))
            .map(|m: &Vec<Message>| m.as_slice());

        let view_focused = self.mode == AppMode::ViewSession;
        self.message_view
            .render(selected_session, messages, view_focused, frame, content_layout[1]);

        // Status bar
        let warning_count = self.warnings.len();
        self.status_bar.render(
            self.mode,
            self.loading,
            &self.search_query,
            self.index_progress,
            warning_count,
            self.filter.is_active(),
            frame,
            main_layout[1],
        );

        // Help overlay
        if self.mode == AppMode::Help {
            render_help_overlay(frame, size);
        }

        // Filter overlay
        if self.mode == AppMode::Filter {
            render_filter_overlay(frame, size, &self.filter);
        }
    }
}

fn push_date_char(date: &mut Option<chrono::NaiveDate>, c: char) {
    if !c.is_ascii_digit() && c != '-' {
        return;
    }
    let mut buf = date.map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());
    buf.push(c);
    *date = chrono::NaiveDate::parse_from_str(&buf, "%Y-%m-%d").ok();
}

fn render_help_overlay(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let header = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let key = Style::default().fg(Color::Yellow);
    let desc = Style::default().fg(Color::White);

    let lines = vec![
        Line::from(Span::styled("Browse Mode", header)),
        Line::from(vec![Span::styled("  j / Down  ", key), Span::styled("Next session", desc)]),
        Line::from(vec![Span::styled("  k / Up    ", key), Span::styled("Previous session", desc)]),
        Line::from(vec![Span::styled("  Enter     ", key), Span::styled("Open session", desc)]),
        Line::from(vec![Span::styled("  g         ", key), Span::styled("Go to top", desc)]),
        Line::from(vec![Span::styled("  G         ", key), Span::styled("Go to bottom", desc)]),
        Line::from(vec![Span::styled("  /         ", key), Span::styled("Search conversations", desc)]),
        Line::from(vec![Span::styled("  f         ", key), Span::styled("Open filter panel", desc)]),
        Line::from(vec![Span::styled("  Tab       ", key), Span::styled("Switch focus", desc)]),
        Line::raw(""),
        Line::from(Span::styled("View Mode", header)),
        Line::from(vec![Span::styled("  j / Down  ", key), Span::styled("Scroll down", desc)]),
        Line::from(vec![Span::styled("  k / Up    ", key), Span::styled("Scroll up", desc)]),
        Line::from(vec![Span::styled("  Ctrl+D    ", key), Span::styled("Page down", desc)]),
        Line::from(vec![Span::styled("  Ctrl+U    ", key), Span::styled("Page up", desc)]),
        Line::from(vec![Span::styled("  g / G     ", key), Span::styled("Top / bottom", desc)]),
        Line::from(vec![Span::styled("  t         ", key), Span::styled("Toggle tool calls", desc)]),
        Line::from(vec![Span::styled("  Esc       ", key), Span::styled("Back to list", desc)]),
        Line::raw(""),
        Line::from(Span::styled("Search Mode", header)),
        Line::from(vec![Span::styled("  Type      ", key), Span::styled("Filter sessions", desc)]),
        Line::from(vec![Span::styled("  Enter     ", key), Span::styled("Open selected", desc)]),
        Line::from(vec![Span::styled("  Esc       ", key), Span::styled("Cancel search", desc)]),
        Line::raw(""),
        Line::from(Span::styled("Filter Panel", header)),
        Line::from(vec![Span::styled("  j / k     ", key), Span::styled("Navigate items", desc)]),
        Line::from(vec![Span::styled("  Space     ", key), Span::styled("Toggle provider", desc)]),
        Line::from(vec![Span::styled("  e         ", key), Span::styled("Edit text field", desc)]),
        Line::from(vec![Span::styled("  Ctrl+C    ", key), Span::styled("Clear all filters", desc)]),
        Line::from(vec![Span::styled("  Esc / f   ", key), Span::styled("Close panel", desc)]),
        Line::raw(""),
        Line::from(Span::styled("Global", header)),
        Line::from(vec![Span::styled("  ?         ", key), Span::styled("Toggle this help", desc)]),
        Line::from(vec![Span::styled("  q         ", key), Span::styled("Quit", desc)]),
        Line::from(vec![Span::styled("  Ctrl+C    ", key), Span::styled("Force quit", desc)]),
    ];

    let help_width = 48;
    let help_height = u16::try_from(lines.len() + 2).unwrap_or(38).min(area.height.saturating_sub(2));
    let x = area.width.saturating_sub(help_width) / 2;
    let y = area.height.saturating_sub(help_height) / 2;

    let help_area = ratatui::layout::Rect::new(x, y, help_width, help_height);

    frame.render_widget(Clear, help_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        help_area,
    );
}

fn render_filter_overlay(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    filter: &FilterState,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let providers = Provider::all();
    let mut lines: Vec<Line> = Vec::new();

    let header = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let selected_style = Style::default().bg(Color::DarkGray);

    lines.push(Line::from(Span::styled("Providers", header)));
    for (i, p) in providers.iter().enumerate() {
        let enabled = filter.provider_enabled.get(p).copied().unwrap_or(true);
        let checkbox = if enabled { "[x]" } else { "[ ]" };
        let mut line = Line::from(vec![
            Span::styled(
                format!("  {checkbox} "),
                Style::default().fg(if enabled { Color::Green } else { Color::Red }),
            ),
            Span::raw(p.as_str()),
        ]);
        if filter.cursor == i {
            line = line.style(selected_style);
        }
        lines.push(line);
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("Filters", header)));

    // Project filter
    let proj_idx = providers.len();
    let proj_value = if filter.project_query.is_empty() {
        "(any)".to_string()
    } else {
        filter.project_query.clone()
    };
    let editing_proj = filter.editing_field == Some(FilterField::Project);
    let proj_suffix = if editing_proj { "█" } else { "" };
    let mut proj_line = Line::from(vec![
        Span::styled("  Project: ", Style::default().fg(Color::Yellow)),
        Span::raw(format!("{proj_value}{proj_suffix}")),
    ]);
    if filter.cursor == proj_idx {
        proj_line = proj_line.style(selected_style);
    }
    lines.push(proj_line);

    // Date from
    let from_idx = proj_idx + 1;
    let from_value = filter
        .date_from
        .map_or_else(|| "(any)".to_string(), |d| d.format("%Y-%m-%d").to_string());
    let editing_from = filter.editing_field == Some(FilterField::DateFrom);
    let from_suffix = if editing_from { "█" } else { "" };
    let mut from_line = Line::from(vec![
        Span::styled("  From:    ", Style::default().fg(Color::Yellow)),
        Span::raw(format!("{from_value}{from_suffix}")),
    ]);
    if filter.cursor == from_idx {
        from_line = from_line.style(selected_style);
    }
    lines.push(from_line);

    // Date to
    let to_idx = from_idx + 1;
    let to_value = filter
        .date_to
        .map_or_else(|| "(any)".to_string(), |d| d.format("%Y-%m-%d").to_string());
    let editing_to = filter.editing_field == Some(FilterField::DateTo);
    let to_suffix = if editing_to { "█" } else { "" };
    let mut to_line = Line::from(vec![
        Span::styled("  To:      ", Style::default().fg(Color::Yellow)),
        Span::raw(format!("{to_value}{to_suffix}")),
    ]);
    if filter.cursor == to_idx {
        to_line = to_line.style(selected_style);
    }
    lines.push(to_line);

    let _ = to_idx; // used above

    let panel_width = 40;
    let panel_height = u16::try_from(lines.len() + 2).unwrap_or(20).min(area.height.saturating_sub(2));
    let x = area.width.saturating_sub(panel_width) / 2;
    let y = area.height.saturating_sub(panel_height) / 2;

    let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);

    frame.render_widget(Clear, panel_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Filter ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        panel_area,
    );
}
