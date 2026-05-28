use std::sync::Arc;
use std::time::Duration;

use crossterm::event::Event;
use ratatui::prelude::Backend;
use ratatui::Terminal;

use crate::embed;
use crate::event::{map_key_event, CrosstermEventSource, EventSource};
use crate::search::SearchIndex;

use super::App;

impl App {
    pub fn run<B: Backend<Error: Send + Sync + 'static>>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> anyhow::Result<()> {
        self.run_with_event_source(terminal, CrosstermEventSource)
    }

    pub fn run_with_event_source<B: Backend<Error: Send + Sync + 'static>>(
        &mut self,
        terminal: &mut Terminal<B>,
        mut events: impl EventSource,
    ) -> anyhow::Result<()> {
        self.open_search_index(SearchIndex::default_index_dir());
        // Default ON when the embedding pipeline is wired up — hybrid is
        // strictly an improvement over lexical when the store is populated.
        // Users can still hit the toggle to compare modes side-by-side.
        self.hybrid.enabled = self.hybrid.available;

        self.load_sessions();
        self.start_indexing();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(Event::Key(key)) = events.poll_event(Duration::from_millis(50))? {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }
                let editing = self.filter.editing_field.is_some();
                if let Some(action) = map_key_event(key, self.mode, editing) {
                    self.dispatch(action);
                }
            }

            while let Ok(action) = self.action_rx.try_recv() {
                self.dispatch(action);
            }

            self.tick();

            if self.lifecycle.should_quit {
                break;
            }
        }

        Ok(())
    }

    pub(super) fn open_search_index(&mut self, index_dir: std::path::PathBuf) {
        self.index_dir = index_dir;
        match SearchIndex::open_or_create(&self.index_dir) {
            Ok(index) => {
                self.search_index = Some(Arc::new(index));
                self.hybrid.available = embed::hybrid_ready(&self.index_dir);
            }
            Err(error) => {
                self.search_index = None;
                self.index_ready = false;
                self.index_progress = None;
                self.hybrid.available = false;
                self.hybrid.enabled = false;
                self.warnings
                    .push(format!("Search index unavailable: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::stars::StarStore;

    use super::*;

    #[test]
    fn open_search_index_records_warning_when_path_is_not_directory() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("index-file");
        std::fs::write(&index_path, b"not a directory").unwrap();
        let mut app = App::with_stars(Vec::new(), Config::default(), StarStore::ephemeral());

        app.open_search_index(index_path);

        assert!(app.search_index.is_none());
        assert_eq!(app.warnings.len(), 1);
        assert!(
            app.warnings[0].contains("Search index unavailable"),
            "unexpected warning: {:?}",
            app.warnings
        );
    }
}
