use crate::model::{CitationRef, Session};

#[derive(Clone, Copy)]
pub enum SelectorShape {
    SessionRefOnly,
    SessionRefOrIdPrefix,
}

#[derive(Debug)]
pub struct SelectedSession<'a> {
    pub session: &'a Session,
    pub source: &'a str,
    pub session_ref: String,
}

#[derive(Debug)]
pub struct SelectedCitation<'a> {
    pub session: &'a Session,
    pub source: &'a str,
    pub citation: CitationRef,
    pub citation_ref: String,
}
