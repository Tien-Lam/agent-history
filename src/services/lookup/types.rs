use crate::model::{CitationRef, Message, Session};

#[derive(Debug)]
pub struct LoadedSession {
    pub session: Session,
    pub source: String,
    pub session_ref: String,
    pub messages: Vec<Message>,
}

#[derive(Debug)]
pub struct LoadedCitationWindow {
    pub session: Session,
    pub source: String,
    pub citation: CitationRef,
    pub citation_ref: String,
    pub messages: Vec<Message>,
    pub start_idx: usize,
    pub target_idx: usize,
    pub total_messages: usize,
}
