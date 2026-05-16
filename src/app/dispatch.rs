use crate::action::Action;
use crate::export::ExportFormat;
use crate::model::Provider;

use super::overlays::push_date_char;
use super::state::{cycle_role, FilterField, FilterState};
use super::{App, AppMode};

mod navigation;
mod search_actions;

impl App {
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
}
