use aghist::action::Action;

use super::*;

#[test]
fn search_input_debounces_until_idle() {
    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('t'));
    app.dispatch(Action::SearchInput('e'));
    app.dispatch(Action::SearchInput('s'));

    assert!(
        app.has_pending_search(),
        "fast typing should leave a pending search"
    );

    app.tick();
    assert!(
        app.has_pending_search(),
        "tick within debounce window should not flush"
    );

    std::thread::sleep(std::time::Duration::from_millis(150));
    app.tick();
    assert!(
        !app.has_pending_search(),
        "tick after debounce window should flush"
    );
}

#[test]
fn search_submit_flushes_pending_search() {
    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('a'));
    assert!(app.has_pending_search());

    app.dispatch(Action::SearchSubmit);
    assert!(
        !app.has_pending_search(),
        "SearchSubmit should flush any pending debounced search"
    );
}

#[test]
fn search_cancel_drops_pending_search() {
    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('z'));
    assert!(app.has_pending_search());

    app.dispatch(Action::SearchCancel);
    assert!(
        !app.has_pending_search(),
        "SearchCancel should drop the pending debounced search"
    );
}
