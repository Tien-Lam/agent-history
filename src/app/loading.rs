use crate::action::Action;
use crate::app::App;

impl App {
    pub fn load_sessions(&mut self) {
        let tx = self.action_tx.clone();
        let mut all_sessions = Vec::new();

        for provider in &*self.providers {
            match provider.discover_sessions() {
                Ok(sessions) => all_sessions.extend(sessions),
                Err(e) => {
                    let _ = tx.send(Action::LoadError(format!("{}: {e}", provider.provider())));
                }
            }
        }

        all_sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        let _ = tx.send(Action::SessionsLoaded(all_sessions));

        while let Ok(action) = self.action_rx.try_recv() {
            self.dispatch(action);
        }
    }
}
