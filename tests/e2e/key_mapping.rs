use aghist::action::Action;
use aghist::event::map_key_event;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

#[test]
fn key_mapping_browse_mode() {
    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(Action::NextItem)
    ));

    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(Action::SelectSession)
    ));

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(Action::CopyResumeCommand)
    ));

    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(Action::Quit)
    ));
}

#[test]
fn key_mapping_view_mode() {
    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(Action::BackToList)
    ));

    let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(Action::ToggleToolCalls)
    ));

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(Action::CopyResumeCommand)
    ));
}

#[test]
fn key_mapping_search_mode() {
    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(Action::SearchCancel)
    ));

    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(Action::SearchSubmit)
    ));

    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(Action::SearchInput('a'))
    ));
}
