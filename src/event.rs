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

pub fn map_key_event(key: KeyEvent, mode: AppMode) -> Option<Action> {
    // Global keybindings
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Some(Action::Quit),
        (KeyCode::Char('q'), _) if mode != AppMode::Search => return Some(Action::Quit),
        _ => {}
    }

    match mode {
        AppMode::Browse => map_browse_key(key),
        AppMode::ViewSession => map_view_key(key),
        AppMode::Search => map_search_key(key),
        AppMode::Help => map_help_key(key),
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
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn map_search_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::SearchCancel),
        KeyCode::Enter => Some(Action::SearchSubmit),
        KeyCode::Backspace => Some(Action::SearchBackspace),
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
