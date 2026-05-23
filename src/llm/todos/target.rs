use crate::model::Provider;

/// Validate a `target_session` string. Accepts a session-style
/// `<provider-slug>/<session-id>` ref, a source-qualified
/// `<source>:<provider-slug>/<session-id>` ref, or a beads-style
/// `<prefix>-<suffix>` id (prefix = 2+ lowercase letters, suffix has at least
/// one digit). Drops anything else so the LLM can't smuggle in arbitrary
/// strings.
pub(in crate::llm) fn sanitize_target_session(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Drop trailing `#turn` if present — the schema says session-level only.
    let head = trimmed.split_once('#').map_or(trimmed, |(h, _)| h);
    if head.is_empty() {
        return None;
    }
    let session_ref = match head.split_once(':') {
        Some((source, rest)) if valid_source_name(source) => rest,
        Some((_source, _rest)) => "",
        None => head,
    };
    if let Some((slug, rest)) = session_ref.split_once('/') {
        if !rest.is_empty() && Provider::all().iter().any(|p| p.slug() == slug) {
            return Some(head.to_string());
        }
    }
    // Beads-style id: at least two lowercase letters, then `-`, then a
    // suffix containing at least one digit. Mirrors `crate::todos::find_bd_refs`.
    if let Some((prefix, suffix)) = head.split_once('-') {
        let prefix_ok = prefix.len() >= 2 && prefix.bytes().all(|b| b.is_ascii_lowercase());
        let suffix_ok = !suffix.is_empty()
            && suffix
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
            && suffix.bytes().any(|b| b.is_ascii_digit());
        if prefix_ok && suffix_ok {
            return Some(head.to_string());
        }
    }
    None
}

fn valid_source_name(source: &str) -> bool {
    let mut bytes = source.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
