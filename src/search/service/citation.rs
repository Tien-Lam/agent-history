use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use crate::model::Session;
use crate::provider::{self, HistoryProvider};
use crate::session_resolver::{qualified_citation_ref, source_for_session};
use crate::session_warnings::SessionLoadWarning;

use super::SearchServiceHit;
use crate::search::HitKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHitCitation {
    pub ref_: String,
    pub turn: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHitCitationResolution {
    pub refs: HashMap<String, SearchHitCitation>,
    pub warnings: Vec<SessionLoadWarning>,
}

pub fn resolve_search_hit_citations<SessionHasher: BuildHasher, SourceHasher: BuildHasher>(
    hits: &[SearchServiceHit],
    sessions: &HashMap<String, &Session, SessionHasher>,
    source_by_session: &HashMap<String, String, SourceHasher>,
    providers: &[Box<dyn HistoryProvider>],
) -> SearchHitCitationResolution {
    let mut refs = HashMap::new();
    let mut warnings = Vec::new();
    let mut seen_sessions = HashSet::new();

    for (hit, _) in hits {
        if !matches!(hit.kind(), HitKind::Message) {
            continue;
        }
        if !seen_sessions.insert(hit.session_key()) {
            continue;
        }
        let Some(session) = sessions.get(hit.session_key()).copied() else {
            continue;
        };
        let source = source_for_session(source_by_session, session);
        let messages = match provider::load_messages_for_session(session, providers) {
            Ok(messages) => messages,
            Err(error) => {
                warnings.push(SessionLoadWarning::new(source, session, error));
                continue;
            }
        };
        for (i, msg) in messages.iter().enumerate() {
            let message_key = session.message_key(i, &msg.id.0);
            let turn = i + 1;
            refs.insert(
                message_key,
                SearchHitCitation {
                    ref_: qualified_citation_ref(
                        source_by_session,
                        session,
                        u32::try_from(turn).unwrap_or(u32::MAX),
                    ),
                    turn,
                },
            );
        }
    }

    SearchHitCitationResolution { refs, warnings }
}
