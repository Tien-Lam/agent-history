use crate::app::App;
use crate::export::ExportFormat;
use std::path::Path;

impl App {
    pub(crate) fn perform_export(&mut self, format: ExportFormat) {
        let session = {
            let Some(idx) = self.session_list.selected_index() else {
                return;
            };
            let display = self.display_sessions();
            match display.get(idx) {
                Some(s) => (*s).clone(),
                None => return,
            }
        };

        let messages = match self.message_cache.get(&session.identity_key()) {
            Some(m) => m.clone(),
            None => return,
        };

        let content = crate::export::export(format, &session, &messages);
        let id_short = session.id.0.get(..8).unwrap_or(&session.id.0);
        let sanitized: String = id_short
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let filename = format!("aghist-{sanitized}.{}", format.extension());

        match crate::export::write_file(Path::new(&filename), &content) {
            Ok(()) => {
                self.status_message = Some(format!("Exported to {filename}"));
            }
            Err(e) => {
                self.warnings.push(format!("Export failed: {e}"));
            }
        }
    }
}
