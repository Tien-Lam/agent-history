use crate::action::Action;
use crate::model::Provider;

use super::super::overlays::push_date_char;
use super::super::state::{cycle_role, FilterField, FilterState};
use super::super::{App, AppMode};

impl App {
    pub(super) fn dispatch_filter(&mut self, action: &Action) {
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
}
