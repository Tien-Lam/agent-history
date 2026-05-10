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
    /// Toggle hybrid (semantic + lexical) search. No-op if the embedding
    /// pipeline isn't ready (missing feature / consent / store).
    ToggleHybrid,
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
    FilterEditDone,
    FilterInput(char),
    FilterBackspace,
    FilterClearAll,
    // Stars / bookmarks
    ToggleStar,
    // Resume
    CopyResumeCommand,
    // Export
    ExportStart,
    ExportNext,
    ExportPrev,
    ExportConfirm,
    ExportCancel,
    // UI
    Resize(u16, u16),
    ToggleToolCalls,
    /// Toggle "raw" mode for tool I/O — drops truncation on tool args,
    /// tool output, and thinking blocks. Only meaningful when tool calls
    /// are also expanded (`ToggleToolCalls`).
    ToggleRawToolOutput,
    ToggleHelp,
    SwitchFocus,
}
