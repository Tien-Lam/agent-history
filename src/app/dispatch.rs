use crate::action::Action;
use crate::export::ExportFormat;

use super::{App, AppMode};

mod filter_actions;
mod navigation;
mod search_actions;

impl App {
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.lifecycle.should_quit = true;
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
                self.lifecycle.loading = false;
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
}
