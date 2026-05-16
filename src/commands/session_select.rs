use std::collections::HashMap;

use aghist::cli_error::ErrorEnvelope;
use aghist::model::{Session, SessionRef};
use aghist::{config, federated};

use super::discovery::{qualified_session_ref, source_for_session};

#[derive(Clone, Copy)]
pub(crate) enum SelectorShape {
    SessionRefOnly,
    SessionRefOrIdPrefix,
}

pub(crate) struct SelectedSession<'a> {
    pub(crate) session: &'a Session,
    pub(crate) session_ref: String,
}

fn split_source_prefix(raw: &str) -> Result<Option<(&str, &str)>, ErrorEnvelope> {
    let slash = raw.find('/');
    let colon = raw.find(':');
    if !matches!((colon, slash), (Some(c), Some(s)) if c < s) {
        return Ok(None);
    }
    let (source, rest) = raw.split_once(':').expect("colon detected above");
    config::validate_source_name(source).map_err(|message| ErrorEnvelope::new("usage", message))?;
    Ok(Some((source, rest)))
}

fn parse_session_ref(raw: &str, full_selector: &str) -> Result<SessionRef, ErrorEnvelope> {
    raw.parse::<SessionRef>().map_err(|e| {
        ErrorEnvelope::new(
            "usage",
            format!("invalid session ref '{full_selector}': {e}"),
        )
        .with_hint("Format: <provider-slug>/<session-id> or <source>:<provider-slug>/<session-id>.")
    })
}

fn target_not_found(selector: &str) -> ErrorEnvelope {
    ErrorEnvelope::new(
        "session-not-found",
        format!("Session not found: {selector}"),
    )
    .with_hint("Run `aghist --list --json` to see available sessions and sources.")
}

fn target_ambiguous(selector: &str, matches: &[&Session], sources: &[String]) -> ErrorEnvelope {
    let mut candidates: Vec<String> = matches
        .iter()
        .zip(sources)
        .take(8)
        .map(|(session, source)| {
            if source == federated::LOCAL_SOURCE {
                session.session_ref().to_string()
            } else {
                format!("{source}:{}", session.session_ref())
            }
        })
        .collect();
    if matches.len() > candidates.len() {
        candidates.push(format!("... and {} more", matches.len() - candidates.len()));
    }
    ErrorEnvelope::new(
        "ambiguous-session",
        format!(
            "session selector '{selector}' matched {} sessions: {}",
            matches.len(),
            candidates.join(", ")
        ),
    )
    .with_hint("Use <source>:<provider>/<session-id> to choose one session explicitly.")
}

fn unique_target<'a>(
    selector: &str,
    matches: &[&'a Session],
    source_by_session: &HashMap<String, String>,
) -> Result<SelectedSession<'a>, ErrorEnvelope> {
    if matches.is_empty() {
        return Err(target_not_found(selector));
    }
    let sources: Vec<String> = matches
        .iter()
        .map(|session| source_for_session(source_by_session, session).to_string())
        .collect();
    if matches.len() > 1 {
        return Err(target_ambiguous(selector, matches, &sources));
    }
    let session = matches[0];
    Ok(SelectedSession {
        session,
        session_ref: qualified_session_ref(source_by_session, session),
    })
}

pub(crate) fn resolve_session_selector<'a>(
    sessions: &'a [Session],
    source_by_session: &HashMap<String, String>,
    selector: &str,
    shape: SelectorShape,
) -> Result<SelectedSession<'a>, ErrorEnvelope> {
    if selector.contains('#') {
        return Err(ErrorEnvelope::new(
            "usage",
            "expected a session ref, not a turn-level citation ref",
        )
        .with_hint("Use `aghist show <ref>` for a single turn, or remove the `#<turn>` suffix."));
    }

    if let Some((source, raw_ref)) = split_source_prefix(selector)? {
        let session_ref = parse_session_ref(raw_ref, selector)?;
        let matches: Vec<&Session> = sessions
            .iter()
            .filter(|session| {
                session.provider == session_ref.provider
                    && session.id == session_ref.session_id
                    && source_for_session(source_by_session, session) == source
            })
            .collect();
        return unique_target(selector, &matches, source_by_session);
    }

    if selector.contains('/') {
        let session_ref = parse_session_ref(selector, selector)?;
        let matches: Vec<&Session> = sessions
            .iter()
            .filter(|session| {
                session.provider == session_ref.provider && session.id == session_ref.session_id
            })
            .collect();
        return unique_target(selector, &matches, source_by_session);
    }

    if matches!(shape, SelectorShape::SessionRefOnly) {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("invalid session ref '{selector}': expected <provider>/<session-id>"),
        )
        .with_hint("Use <source>:<provider>/<session-id> for remote-source sessions."));
    }

    let exact_matches: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.id.0 == selector)
        .collect();
    if !exact_matches.is_empty() {
        return unique_target(selector, &exact_matches, source_by_session);
    }
    let prefix_matches: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.id.0.starts_with(selector))
        .collect();
    unique_target(selector, &prefix_matches, source_by_session)
}
