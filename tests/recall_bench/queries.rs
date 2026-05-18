use std::fs;
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct LabeledQuery {
    pub(crate) id: String,
    pub(crate) tag: String,
    pub(crate) query: String,
    pub(crate) expected_session_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct QueryFile {
    queries: Vec<LabeledQuery>,
}

pub(crate) fn load_queries() -> Vec<LabeledQuery> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bench_recall/queries.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let file: QueryFile = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    file.queries
}
