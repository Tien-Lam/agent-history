use tantivy::query::{BooleanQuery, Occur, Query};

mod filters;
mod hit;
mod hybrid;
mod lexical;
mod session_filter;

fn combined_query(mut clauses: Vec<(Occur, Box<dyn Query>)>) -> Box<dyn Query> {
    if clauses.len() == 1 {
        clauses.remove(0).1
    } else {
        Box::new(BooleanQuery::new(clauses))
    }
}
