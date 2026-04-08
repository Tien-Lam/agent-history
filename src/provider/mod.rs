pub mod claude;
pub mod codex;
pub mod copilot;
pub mod gemini;
pub mod opencode;
pub mod registry;

use crate::error::Result;
use crate::model::{Message, Session};

/// A source of AI agent conversation history.
pub trait Provider: Send + Sync {
    /// Short lowercase name: "claude", "codex", etc.
    fn name(&self) -> &'static str;

    /// Whether this provider's data directory exists on disk.
    fn is_available(&self) -> bool;

    /// Discover sessions without fully parsing messages.
    /// Reads only metadata and the first few lines per file.
    fn discover_sessions(&self) -> Result<Vec<Session>>;

    /// Fully parse all messages for a given session.
    fn load_messages(&self, session: &Session) -> Result<Vec<Message>>;
}
