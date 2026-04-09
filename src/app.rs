use std::num::NonZeroUsize;
use std::time::Duration;

use chrono::Utc;
use crossterm::event::Event;
use lru::LruCache;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Backend;
use ratatui::Terminal;

use crate::action::Action;
use crate::event::{map_key_event, poll_event};
use crate::model::{Message, Provider, Session, SessionId};
use crate::provider::HistoryProvider;
use crate::ui::message_view::MessageViewComponent;
use crate::ui::session_list::SessionListComponent;
use crate::ui::status_bar::StatusBarComponent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browse,
    ViewSession,
    Search,
    Help,
}

pub struct App {
    sessions: Vec<Session>,
    message_cache: LruCache<String, Vec<Message>>,
    mode: AppMode,
    loading: bool,
    should_quit: bool,

    session_list: SessionListComponent,
    message_view: MessageViewComponent,
    status_bar: StatusBarComponent,

    providers: Vec<Box<dyn HistoryProvider>>,
    action_rx: crossbeam_channel::Receiver<Action>,
    action_tx: crossbeam_channel::Sender<Action>,
}

impl App {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>) -> Self {
        let (action_tx, action_rx) = crossbeam_channel::unbounded();

        Self {
            sessions: Vec::new(),
            message_cache: LruCache::new(NonZeroUsize::new(20).unwrap()),
            mode: AppMode::Browse,
            loading: true,
            should_quit: false,

            session_list: SessionListComponent::new(),
            message_view: MessageViewComponent::new(),
            status_bar: StatusBarComponent::new(),

            providers,
            action_rx,
            action_tx,
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
        self.load_sessions();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(Event::Key(key)) = poll_event(Duration::from_millis(50))? {
                if let Some(action) = map_key_event(key, self.mode) {
                    self.dispatch(action);
                }
            }

            // Drain background actions
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

        for provider in &self.providers {
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

    #[allow(clippy::too_many_lines)]
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }

            // Navigation
            Action::NextItem => {
                if let Some(selected) = self.session_list.selected_index() {
                    if selected + 1 < self.sessions.len() {
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
            Action::SelectSession => {
                if let Some(selected) = self.session_list.selected_index() {
                    if selected < self.sessions.len() {
                        let session_id = self.sessions[selected].id.0.clone();
                        let source_path = self.sessions[selected].source_path.clone();
                        let provider = self.sessions[selected].provider;
                        self.load_messages_cached(&session_id, &source_path, provider);
                        self.message_view.reset_scroll();
                        self.mode = AppMode::ViewSession;
                    }
                }
            }
            Action::BackToList | Action::SearchCancel => {
                self.mode = AppMode::Browse;
            }
            Action::GoToTop => match self.mode {
                AppMode::Browse => {
                    if !self.sessions.is_empty() {
                        self.session_list.state.select(Some(0));
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = 0;
                }
                _ => {}
            },
            Action::GoToBottom => match self.mode {
                AppMode::Browse => {
                    if !self.sessions.is_empty() {
                        self.session_list
                            .state
                            .select(Some(self.sessions.len() - 1));
                    }
                }
                AppMode::ViewSession => {
                    self.message_view.scroll_offset = u16::MAX;
                }
                _ => {}
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

            // Tool calls
            Action::ToggleToolCalls => {
                self.message_view.show_tool_calls = !self.message_view.show_tool_calls;
            }

            // Help
            Action::ToggleHelp => {
                self.mode = if self.mode == AppMode::Help {
                    AppMode::Browse
                } else {
                    AppMode::Help
                };
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
            Action::LoadError(_msg) => {
                // Future: show in status bar
            }

            // Search (stubbed for Phase 1)
            Action::SearchStart => {
                self.mode = AppMode::Search;
            }

            // Unhandled for now
            Action::SearchInput(_)
            | Action::SearchBackspace
            | Action::SearchSubmit
            | Action::Resize(_, _)
            | Action::SwitchFocus => {}
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

        // Build a temporary Session just for loading
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
                Ok(messages) => {
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

        // Session list
        let list_focused = self.mode == AppMode::Browse;
        self.session_list
            .render(&self.sessions, list_focused, frame, content_layout[0]);

        // Message view
        let selected = self.session_list.selected_index();
        let session = selected.and_then(|i| self.sessions.get(i));
        let messages = session
            .and_then(|s| self.message_cache.get(&s.id.0))
            .map(|m: &Vec<Message>| m.as_slice());

        let view_focused = self.mode == AppMode::ViewSession;
        self.message_view
            .render(session, messages, view_focused, frame, content_layout[1]);

        // Status bar
        self.status_bar
            .render(self.mode, self.loading, frame, main_layout[1]);

        // Help overlay
        if self.mode == AppMode::Help {
            render_help_overlay(frame, size);
        }
    }
}

fn render_help_overlay(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let help_text = "\
Keybindings:

  j / ↓       Next session / scroll down
  k / ↑       Previous session / scroll up
  Enter       Open selected session
  Esc         Back to session list
  g           Go to top
  G           Go to bottom
  t           Toggle tool call details
  /           Search (coming soon)
  ?           Toggle this help
  q           Quit
  Ctrl+C      Force quit";

    let help_width = 48;
    let help_height = 16;
    let x = area.width.saturating_sub(help_width) / 2;
    let y = area.height.saturating_sub(help_height) / 2;

    let help_area = ratatui::layout::Rect::new(x, y, help_width, help_height);

    frame.render_widget(Clear, help_area);
    frame.render_widget(
        Paragraph::new(help_text).block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        help_area,
    );
}
