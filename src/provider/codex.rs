use crate::error::Result;
use crate::model::{Message, Session};
use crate::provider::Provider;

pub struct CodexProvider;

impl Provider for CodexProvider {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn is_available(&self) -> bool {
        false
    }

    fn discover_sessions(&self) -> Result<Vec<Session>> {
        Ok(Vec::new())
    }

    fn load_messages(&self, _session: &Session) -> Result<Vec<Message>> {
        Ok(Vec::new())
    }
}
