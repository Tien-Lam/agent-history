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
        self.index_dir = SearchIndex::default_index_dir();
        self.search_index = SearchIndex::open_or_create(&self.index_dir)
            .map(Arc::new)
            .ok();
        self.hybrid_available = embed::hybrid_ready(&self.index_dir);
        // Default ON when the embedding pipeline is wired up — hybrid is
        // strictly an improvement over lexical when the store is populated.
        // Users can still hit the toggle to compare modes side-by-side.
        self.hybrid_enabled = self.hybrid_available;

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

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }
}
