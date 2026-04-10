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
    // Index (from background thread)
    IndexProgress(usize, usize),
    IndexReady,
    // Data (from background threads)
    SessionsLoaded(Vec<Session>),
    MessagesLoaded(SessionId, Vec<Message>),
    LoadError(String),
    // Filter
    ToggleFilter,
    FilterNext,
    FilterPrev,
    FilterToggle,
    FilterEdit,
    FilterInput(char),
    FilterBackspace,
    FilterClearAll,
    // Export
    ExportStart,
    ExportNext,
    ExportPrev,
    ExportConfirm,
    ExportCancel,
    // UI
    Resize(u16, u16),
    ToggleToolCalls,
    ToggleHelp,
    SwitchFocus,
}
