use crate::error::Result;
use crate::model::{Message, Session};
use crate::provider::Provider;

pub struct CopilotProvider;

impl Provider for CopilotProvider {
    fn name(&self) -> &'static str {
        "copilot"
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
