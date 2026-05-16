use crate::action::Action;

use super::super::{App, AppMode};

impl App {
    pub(super) fn dispatch_navigation(&mut self, action: &Action) {
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
                // Auto-expand tool calls when entering raw mode; a raw toggle
                // that produces no visible output would just confuse.
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
}
