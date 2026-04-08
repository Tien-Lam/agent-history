use crate::model::{Message, Session, SessionId};

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    // Navigation
    NextItem,
    PrevItem,
    SelectSession,
    BackToList,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    GoToTop,
    GoToBottom,
    // Search (future)
    SearchStart,
    SearchInput(char),
    SearchBackspace,
    SearchSubmit,
    SearchCancel,
    // Data (from background threads)
    SessionsLoaded(Vec<Session>),
    MessagesLoaded(SessionId, Vec<Message>),
    LoadError(String),
    // UI
    Resize(u16, u16),
    ToggleToolCalls,
    ToggleHelp,
    SwitchFocus,
}
