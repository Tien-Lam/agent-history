use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::model::{Provider, Session};

use super::error::ResolutionError;
use super::parse::{parse_citation_ref, parse_session_ref};
use super::refs::{qualified_citation_ref, qualified_session_ref, source_for_session};
use super::source::{split_valid_source_prefix, LookupSource};
use super::types::{SelectedCitation, SelectedSession, SelectorShape};

pub struct SessionResolver<'a, S = std::collections::hash_map::RandomState> {
    sessions: &'a [Session],
    source_by_session: &'a HashMap<String, String, S>,
}

impl<'a, S: BuildHasher> SessionResolver<'a, S> {
    pub fn new(sessions: &'a [Session], source_by_session: &'a HashMap<String, String, S>) -> Self {
        Self {
            sessions,
            source_by_session,
        }
    }

    pub fn source_for_session(&self, session: &'a Session) -> &'a str {
        source_for_session(self.source_by_session, session)
    }

    pub fn qualified_session_ref(&self, session: &Session) -> String {
        qualified_session_ref(self.source_by_session, session)
    }

    pub fn qualified_citation_ref(&self, session: &Session, turn: u32) -> String {
        qualified_citation_ref(self.source_by_session, session, turn)
    }

    pub fn resolve_session_selector(
        &self,
        selector: &str,
        shape: SelectorShape,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if selector.contains('#') {
            return Err(ResolutionError::TurnRefForSession);
        }

        if let Some((source, raw_ref)) = split_valid_source_prefix(selector)? {
            let session_ref = parse_session_ref(raw_ref, selector)?;
            let matches: Vec<&Session> = self
                .sessions
                .iter()
                .filter(|session| {
                    session.provider == session_ref.provider
                        && session.id == session_ref.session_id
                        && source.matches(self.source_for_session(session))
                })
                .collect();
            return self.unique_target(selector, &matches);
        }

        if selector.contains('/') {
            let session_ref = parse_session_ref(selector, selector)?;
            let matches: Vec<&Session> = self
                .sessions
                .iter()
                .filter(|session| {
                    session.provider == session_ref.provider && session.id == session_ref.session_id
                })
                .collect();
            return self.unique_target(selector, &matches);
        }

        if matches!(shape, SelectorShape::SessionRefOnly) {
            return Err(ResolutionError::SessionRefRequired(selector.to_string()));
        }

        self.find_by_id_prefix(selector, None, LookupSource::Any)
    }

    pub fn resolve_citation_selector(
        &self,
        selector: &str,
    ) -> Result<SelectedCitation<'a>, ResolutionError> {
        let (source, citation) =
            if let Some((source, raw_ref)) = split_valid_source_prefix(selector)? {
                (Some(source), parse_citation_ref(raw_ref, selector)?)
            } else {
                (None, parse_citation_ref(selector, selector)?)
            };

        let matches: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session.provider == citation.provider
                    && session.id == citation.session_id
                    && source
                        .map_or(LookupSource::Any, |source| source)
                        .matches(self.source_for_session(session))
            })
            .collect();
        let target = self.unique_target(selector, &matches)?;
        Ok(SelectedCitation {
            session: target.session,
            source: target.source,
            citation_ref: format!("{}#{}", target.session_ref, citation.turn),
            citation,
        })
    }

    pub fn find_by_id_prefix(
        &self,
        session_id: &str,
        provider_filter: Option<Provider>,
        source_filter: LookupSource<'_>,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        let candidates: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| provider_filter.is_none_or(|want| session.provider == want))
            .filter(|session| source_filter.matches(self.source_for_session(session)))
            .filter(|session| session.id.0 == session_id || session.id.0.starts_with(session_id))
            .collect();

        let exact: Vec<&Session> = candidates
            .iter()
            .copied()
            .filter(|session| session.id.0 == session_id)
            .collect();
        let matches = if exact.is_empty() { candidates } else { exact };
        self.unique_target(&source_filter.qualify_selector(session_id), &matches)
    }

    pub fn find_exact(
        &self,
        provider: Provider,
        session_id: &str,
        source_filter: LookupSource<'_>,
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        let selector =
            source_filter.qualify_selector(&format!("{}/{}", provider.slug(), session_id));
        let matches: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session.provider == provider
                    && session.id.0 == session_id
                    && source_filter.matches(self.source_for_session(session))
            })
            .collect();
        self.unique_target(&selector, &matches)
    }

    fn unique_target(
        &self,
        selector: &str,
        matches: &[&'a Session],
    ) -> Result<SelectedSession<'a>, ResolutionError> {
        if matches.is_empty() {
            return Err(ResolutionError::NotFound(selector.to_string()));
        }
        if matches.len() > 1 {
            return Err(self.target_ambiguous(selector, matches));
        }
        let session = matches[0];
        Ok(SelectedSession {
            session,
            source: self.source_for_session(session),
            session_ref: self.qualified_session_ref(session),
        })
    }

    fn target_ambiguous(&self, selector: &str, matches: &[&Session]) -> ResolutionError {
        let mut candidates: Vec<String> = matches
            .iter()
            .take(8)
            .map(|session| self.qualified_session_ref(session))
            .collect();
        if matches.len() > candidates.len() {
            candidates.push(format!("... and {} more", matches.len() - candidates.len()));
        }
        ResolutionError::Ambiguous {
            selector: selector.to_string(),
            count: matches.len(),
            candidates,
        }
    }
}
