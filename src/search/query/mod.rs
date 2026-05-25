use tantivy::query::{BooleanQuery, Occur, Query};

mod filters;
mod hit;
mod hybrid;
mod lexical;
mod session_filter;

fn combined_query(clauses: Vec<(Occur, Box<dyn Query>)>) -> Box<dyn Query> {
    if clauses.len() == 1 {
        match clauses.into_iter().next() {
            Some((_, query)) => query,
            None => Box::new(BooleanQuery::new(Vec::new())),
        }
    } else {
        Box::new(BooleanQuery::new(clauses))
    }
}
