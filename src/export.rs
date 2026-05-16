use crate::metadata::Note;
use crate::model::{Message, Session};

mod html;
mod json;
mod markdown;
mod notes;

pub use html::{to_html, to_html_with_notes};
pub use json::{to_json, to_json_with_notes};
pub use markdown::{to_markdown, to_markdown_with_notes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Html,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Html => "html",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Markdown, Self::Json, Self::Html]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::Html => "HTML",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            "html" => Ok(Self::Html),
            _ => Err(format!("unknown format '{s}' (expected: md, json, html)")),
        }
    }
}

pub fn export(format: ExportFormat, session: &Session, messages: &[Message]) -> String {
    export_with_notes(format, session, messages, &[])
}

/// Same as [`export`], but with `notes` injected inline at their citation refs.
///
/// Notes are partitioned by `session_ref`: those without a `#turn` suffix are
/// session-level and render once near the metadata header; those with a turn
/// render right after the corresponding message (1-based turn = message index
/// + 1). Notes whose ref doesn't match this session are silently ignored.
pub fn export_with_notes(
    format: ExportFormat,
    session: &Session,
    messages: &[Message],
    notes: &[Note],
) -> String {
    match format {
        ExportFormat::Markdown => to_markdown_with_notes(session, messages, notes),
        ExportFormat::Json => to_json_with_notes(session, messages, notes),
        ExportFormat::Html => to_html_with_notes(session, messages, notes),
    }
}
