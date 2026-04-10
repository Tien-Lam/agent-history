use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use aghist::action::Action;
use aghist::app::App;
use aghist::config::Config;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
    ]
}

fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

fn render_to_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let area = buf.area;
    let mut result = String::new();
    for y in area.y..area.y + area.height {
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        result.push_str(line.trim_end());
        result.push('\n');
    }
    result
}

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
