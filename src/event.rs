use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::AppMode;

pub fn poll_event(timeout: std::time::Duration) -> std::io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

pub fn map_key_event(key: KeyEvent, mode: AppMode, filter_editing: bool) -> Option<Action> {
    // Global keybindings (only in modes where they make sense)
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) if !filter_editing => {
            return Some(Action::Quit);
        }
        (KeyCode::Char('q'), _)
            if matches!(mode, AppMode::Browse | AppMode::ViewSession) =>
        {
            return Some(Action::Quit);
        }
        _ => {}
    }

    match mode {
        AppMode::Browse => map_browse_key(key),
        AppMode::ViewSession => map_view_key(key),
        AppMode::Search => map_search_key(key),
        AppMode::Help => map_help_key(key),
        AppMode::Filter => map_filter_key(key, filter_editing),
        AppMode::ExportMenu => map_export_key(key),
    }
}

fn map_browse_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::NextItem),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::PrevItem),
        KeyCode::Enter => Some(Action::SelectSession),
        KeyCode::Char('g') => Some(Action::GoToTop),
        KeyCode::Char('G') => Some(Action::GoToBottom),
        KeyCode::Char('/') => Some(Action::SearchStart),
        KeyCode::Char('f') => Some(Action::ToggleFilter),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Tab => Some(Action::SwitchFocus),
        _ => None,
    }
}

fn map_view_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::BackToList),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::ScrollDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::ScrollUp),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageUp)
        }
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char('g') => Some(Action::GoToTop),
        KeyCode::Char('G') => Some(Action::GoToBottom),
        KeyCode::Char('t') => Some(Action::ToggleToolCalls),
        KeyCode::Char('e') => Some(Action::ExportStart),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn map_search_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::SearchCancel),
        KeyCode::Enter => Some(Action::SearchSubmit),
        KeyCode::Backspace => Some(Action::SearchBackspace),
        KeyCode::Up => Some(Action::PrevItem),
        KeyCode::Down => Some(Action::NextItem),
        KeyCode::Char(c) => Some(Action::SearchInput(c)),
        _ => None,
    }
}

fn map_help_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn map_filter_key(key: KeyEvent, editing: bool) -> Option<Action> {
    if editing {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::FilterEditDone),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Char(c) => Some(Action::FilterInput(c)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('f') => Some(Action::ToggleFilter),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::FilterNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::FilterPrev),
        KeyCode::Char(' ') | KeyCode::Enter => Some(Action::FilterToggle),
        KeyCode::Char('e') => Some(Action::FilterEdit),
        KeyCode::Backspace => Some(Action::FilterBackspace),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::FilterClearAll)
        }
        KeyCode::Char(c) => Some(Action::FilterInput(c)),
        _ => None,
    }
}

fn map_export_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExportCancel),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::ExportNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::ExportPrev),
        KeyCode::Enter => Some(Action::ExportConfirm),
        _ => None,
    }
}
