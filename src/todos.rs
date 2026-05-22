//! Heuristic extraction of unresolved TODOs / follow-ups / open beads
//! from session transcripts.
//!
//! The default path is deliberately heuristic-first: regex-style scans,
//! deterministic output, and no network calls. Agents can chain follow-up
//! extraction (e.g. drop refs whose `bd show` reports closed) on top of the
//! JSON shape produced here. The CLI also has an optional LLM refinement mode
//! layered above these candidates.
//!
//! Heuristics scanned per line:
//!   * `TODO` keyword (uppercase, word-bounded — avoids `todo` in prose
//!     and the literal tool name `TodoWrite` which appears constantly in
//!     Claude Code transcripts).
//!   * `follow-up` / `follow up` (case-insensitive).
//!   * `come back to` (case-insensitive).
//!   * `we should` (case-insensitive).
//!   * Beads-style refs `<prefix>-<suffix>` where prefix is 2+ lowercase
//!     letters and suffix has at least one digit (e.g. `ahist-y3o.7.2`,
//!     `gt-abc1`). Word-bounded so `follow-up` doesn't match.
//!
//! Each match becomes one [`TodoCandidate`] keyed by a [`CitationRef`] so
//! the caller can quote, deep-link, or `aghist show` the originating turn.

use serde::Serialize;

use crate::model::{CitationRef, Role};

mod scan;

#[cfg(test)]
mod tests;

pub use scan::extract_from_messages;

/// What kind of heuristic surfaced a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoKind {
    Todo,
    FollowUp,
    ComeBackTo,
    WeShould,
    BdRef,
}

impl TodoKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::FollowUp => "follow-up",
            Self::ComeBackTo => "come-back-to",
            Self::WeShould => "we-should",
            Self::BdRef => "bd-ref",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "todo" => Some(Self::Todo),
            "follow-up" | "followup" => Some(Self::FollowUp),
            "come-back-to" | "comebackto" => Some(Self::ComeBackTo),
            "we-should" | "weshould" => Some(Self::WeShould),
            "bd-ref" | "bdref" | "bd" => Some(Self::BdRef),
            _ => None,
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Todo,
        Self::FollowUp,
        Self::ComeBackTo,
        Self::WeShould,
        Self::BdRef,
    ];
}

/// One surfaced candidate. `snippet` is the raw matched line, trimmed.
#[derive(Debug, Clone, Serialize)]
pub struct TodoCandidate {
    pub citation: CitationRef,
    pub kind: TodoKind,
    pub snippet: String,
    pub role: Role,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Set only for [`TodoKind::BdRef`] — the captured ID (e.g. `ahist-y3o.7.2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bd_id: Option<String>,
}
