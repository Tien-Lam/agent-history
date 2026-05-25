use aghist::model::{Provider, Role};
use aghist::schema_fragments::{
    FILTER_PROJECT_MAX_BYTES, FILTER_TIMESTAMP_MAX_BYTES, METADATA_NOTE_FILTER_MAX_BYTES,
    METADATA_TAG_MAX_BYTES,
};
use aghist::search::SearchFilters;
use chrono::{DateTime, Utc};
use clap::Args;

use super::resolvers::parse_provider_slug;

// Common filter flags shared between `--list` and `search`.
//
// `--has-tool-call` filters at the message level (drops messages without a
// tool invocation); other flags filter at the session or message level
// depending on the subcommand. `--since`/`--until` accept RFC 3339 dates only.
//
// Doc comment intentionally suppressed: clap promotes a flattened struct's
// doc comment to the parent's `about` text, overriding our explicit
// `about = "Browse and search..."` on `Cli`.
#[derive(Debug, Clone, Args)]
pub(crate) struct FilterArgs {
    /// Restrict to a single provider slug, e.g. `claude-code` or `codex-cli`.
    #[arg(long, global = true, value_parser = parse_provider_slug, value_name = "SLUG")]
    pub(crate) provider: Option<Provider>,

    /// RFC 3339 lower bound on message/session timestamp (inclusive).
    /// Example: `--since 2025-01-01T00:00:00Z`.
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    pub(crate) since: Option<DateTime<Utc>>,

    /// RFC 3339 upper bound on message/session timestamp (inclusive).
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    pub(crate) until: Option<DateTime<Utc>>,

    /// Substring match against the session's project name (case-insensitive).
    #[arg(long, global = true, value_name = "NAME", value_parser = parse_project_filter)]
    pub(crate) project: Option<String>,

    /// Restrict to messages with this role: `user`, `assistant`, or `tool`.
    #[arg(long, global = true, value_parser = parse_role_slug, value_name = "ROLE")]
    pub(crate) role: Option<Role>,

    /// Keep only messages (or sessions containing messages) that include a
    /// tool invocation. Has no effect on session-level lookups that do not
    /// load message content.
    #[arg(long, global = true)]
    pub(crate) has_tool_call: bool,

    /// Keep only sessions that have a user note whose body contains this
    /// substring (case-insensitive). Matches notes attached to the session
    /// itself or to any of its turns. Notes live in the metadata sidecar
    /// (`~/.local/share/aghist/metadata.db`; `AGHIST_METADATA_DB` overrides).
    #[arg(long, global = true, value_name = "SUBSTR", value_parser = parse_note_filter)]
    pub(crate) note: Option<String>,

    /// Keep only sessions that have this exact tag attached (session-level
    /// OR on any of its turns). Tags live in the same metadata sidecar.
    #[arg(long, global = true, value_name = "NAME", value_parser = parse_tag_filter)]
    pub(crate) tag: Option<String>,

    /// Keep only sessions that have at least one star (session-level OR on
    /// any of its turns). Stars live in the same metadata sidecar.
    #[arg(long, global = true)]
    pub(crate) starred: bool,
}

impl FilterArgs {
    pub(crate) fn to_search_filters(&self) -> SearchFilters {
        SearchFilters {
            provider: self.provider,
            since: self.since,
            until: self.until,
            project: self.project.clone(),
            role: self.role,
            has_tool_call: self.has_tool_call,
        }
    }

    pub(crate) fn has_metadata_filter(&self) -> bool {
        self.note.is_some() || self.tag.is_some() || self.starred
    }

    pub(crate) fn project_needle(&self) -> Option<String> {
        self.project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty())
    }
}

fn parse_role_slug(raw: &str) -> Result<Role, String> {
    Role::from_slug(raw)
        .ok_or_else(|| format!("unknown role '{raw}'. Valid: user, assistant, system, tool"))
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, String> {
    if raw.len() > FILTER_TIMESTAMP_MAX_BYTES {
        return Err(format!(
            "timestamp filter must be at most {FILTER_TIMESTAMP_MAX_BYTES} bytes"
        ));
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC 3339 timestamp '{raw}': {e}"))
}

fn parse_project_filter(raw: &str) -> Result<String, String> {
    parse_bounded_filter(raw, "project filter", FILTER_PROJECT_MAX_BYTES)
}

fn parse_note_filter(raw: &str) -> Result<String, String> {
    parse_bounded_filter(raw, "note filter", METADATA_NOTE_FILTER_MAX_BYTES)
}

fn parse_tag_filter(raw: &str) -> Result<String, String> {
    parse_bounded_filter(raw, "tag filter", METADATA_TAG_MAX_BYTES)
}

fn parse_bounded_filter(raw: &str, label: &str, max: usize) -> Result<String, String> {
    if raw.len() > max {
        Err(format!("{label} must be at most {max} bytes"))
    } else {
        Ok(raw.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rfc3339_rejects_oversized_values() {
        let raw = "2".repeat(FILTER_TIMESTAMP_MAX_BYTES + 1);
        let err = parse_rfc3339(&raw).unwrap_err();
        assert!(err.contains(&FILTER_TIMESTAMP_MAX_BYTES.to_string()));
    }

    #[test]
    fn filter_string_parsers_reject_oversized_values() {
        assert!(
            parse_project_filter(&"p".repeat(FILTER_PROJECT_MAX_BYTES + 1))
                .unwrap_err()
                .contains(&FILTER_PROJECT_MAX_BYTES.to_string())
        );
        assert!(
            parse_note_filter(&"n".repeat(METADATA_NOTE_FILTER_MAX_BYTES + 1))
                .unwrap_err()
                .contains(&METADATA_NOTE_FILTER_MAX_BYTES.to_string())
        );
        assert!(parse_tag_filter(&"t".repeat(METADATA_TAG_MAX_BYTES + 1))
            .unwrap_err()
            .contains(&METADATA_TAG_MAX_BYTES.to_string()));
    }
}
