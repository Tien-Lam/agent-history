mod common;

use aghist::action::Action;
use aghist::app::App;
use aghist::config::Config;
use aghist::provider::HistoryProvider;

use common::helpers::{all_providers, make_terminal, render_to_text};

// ─── Session list rendering ────────────────────────────────────────────────────

#[test]
fn snapshot_browse_mode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

#[test]
fn snapshot_browse_mode_selection_moved() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::NextItem);
    app.dispatch(Action::NextItem);
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

// ─── Message view with role colors ─────────────────────────────────────────────

#[test]
fn snapshot_message_view() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::SelectSession);
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

#[test]
fn snapshot_message_view_with_tool_calls() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::SelectSession);
    app.dispatch(Action::ToggleToolCalls);
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

// ─── Help overlay ──────────────────────────────────────────────────────────────

#[test]
fn snapshot_help_overlay() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::ToggleHelp);
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

// ─── Empty state ───────────────────────────────────────────────────────────────

#[test]
fn snapshot_empty_state() {
    let providers: Vec<Box<dyn HistoryProvider>> = Vec::new();
    let mut app = App::new(providers, Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

// ─── Filter panel ──────────────────────────────────────────────────────────────

#[test]
fn snapshot_filter_panel() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::ToggleFilter);
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}

// ─── Search mode ───────────────────────────────────────────────────────────────

#[test]
fn snapshot_search_mode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('t'));
    app.dispatch(Action::SearchInput('e'));
    app.dispatch(Action::SearchInput('s'));
    app.dispatch(Action::SearchInput('t'));
    terminal.draw(|frame| app.render(frame)).unwrap();

    insta::assert_snapshot!(render_to_text(&terminal));
}
