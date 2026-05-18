use std::ops::Bound;

use tantivy::query::{Occur, Query, RangeQuery, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use crate::search::document::field_text;
use crate::search::index::SearchIndex;
use crate::search::types::SearchFilters;

impl SearchIndex {
    pub(crate) fn add_filter_clauses(
        &self,
        clauses: &mut Vec<(Occur, Box<dyn Query>)>,
        filters: &SearchFilters,
    ) {
        if let Some(provider) = filters.provider {
            let term = Term::from_field_text(self.fields.provider, provider.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if let Some(role) = filters.role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.since.is_some() || filters.until.is_some() {
            let lower = filters.since.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            let upper = filters.until.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }
    }

    pub(crate) fn project_filter_needle(filters: &SearchFilters) -> Option<String> {
        filters
            .project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty())
    }

    pub(crate) fn matches_project_filter(
        &self,
        doc: &TantivyDocument,
        needle: Option<&str>,
    ) -> bool {
        let Some(needle) = needle else {
            return true;
        };
        field_text(doc, self.fields.project_raw)
            .to_lowercase()
            .contains(needle)
    }
}
