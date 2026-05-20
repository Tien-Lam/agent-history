use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::federated::LOCAL_SOURCE;
use crate::model::{QualifiedCitationRef, Session};

pub fn source_for_session<'a, S: BuildHasher>(
    source_by_session: &'a HashMap<String, String, S>,
    session: &Session,
) -> &'a str {
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
}

pub fn qualified_session_ref<S: BuildHasher>(
    source_by_session: &HashMap<String, String, S>,
    session: &Session,
) -> String {
    let session_ref = session.session_ref().to_string();
    let source = source_for_session(source_by_session, session);
    if source == LOCAL_SOURCE {
        session_ref
    } else {
        format!("{source}:{session_ref}")
    }
}

/// Build the canonical metadata key for a local session.
pub fn session_metadata_key(session: &Session) -> String {
    session.session_ref().to_string()
}

/// Build the metadata key for a federated session. Local sessions keep the
/// legacy unqualified shape; remote sessions use
/// `<source>:<provider-slug>/<id>`.
pub fn qualified_session_metadata_key(session: &Session, source: &str) -> String {
    let raw = session_metadata_key(session);
    if source == LOCAL_SOURCE {
        raw
    } else {
        format!("{source}:{raw}")
    }
}

pub fn qualified_citation_ref<S: BuildHasher>(
    source_by_session: &HashMap<String, String, S>,
    session: &Session,
    turn: u32,
) -> String {
    let source = source_for_session(source_by_session, session);
    let Some(citation) = session.citation_ref(turn) else {
        let raw_ref = format!("{}/{}#{turn}", session.provider.slug(), session.id.0);
        return if source == LOCAL_SOURCE {
            raw_ref
        } else {
            format!("{source}:{raw_ref}")
        };
    };
    QualifiedCitationRef::new(
        (source != LOCAL_SOURCE).then(|| source.to_string()),
        citation,
    )
    .to_string()
}
