//! Recall@10 / MRR benchmark for the search index.
//!
//! Builds a mixed-provider synthetic corpus, runs a labeled query set against
//! it, and reports recall@10 + MRR + coarse latency gates for each retrieval
//! strategy:
//!
//! - **lexical**: BM25 via Tantivy, exactly what `aghist search` uses today.
//! - **semantic**: cosine similarity on a deterministic hashed bag-of-words
//!   embedding. This is *not* the production embedder (fastembed / MiniLM is
//!   feature-gated and downloads a model on first run); it's a stand-in that
//!   gives us a non-lexical signal we can run anywhere, including CI without
//!   network. The harness can be re-run with the production ranker by swapping
//!   the `SemanticRanker` impl.
//! - **hybrid**: Reciprocal Rank Fusion (RRF) of the lexical and semantic
//!   ranked lists with `k = 60`.
//!
//! ## How to run
//!
//! ```text
//! cargo test --test recall_bench -- --nocapture
//! AGHIST_BENCH_WRITE_REPORT=1 cargo test --test recall_bench -- --nocapture
//! ```
//!
//! Setting `AGHIST_BENCH_WRITE_REPORT=1` writes an ignored local report to
//! `docs/SEARCH_BENCH.md` with freshly measured numbers. The default test
//! asserts conservative quality and latency floors so the suite stays fast.

// Pedantic lints we deliberately ignore in this bench:
// - cast_precision_loss / cast_possible_truncation: usize -> f32 for
//   recall/MRR arithmetic with values far below 2^23.
// - format_push_string: writing a markdown table with `push_str(&format!())`
//   is more readable than juggling `writeln!` for a one-shot report.
// - doc_markdown: model names like "MiniLM" don't need backticks in prose.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    clippy::doc_markdown
)]

mod common;
#[path = "recall_bench/corpus.rs"]
mod corpus;
#[path = "recall_bench/metrics.rs"]
mod metrics;
#[path = "recall_bench/queries.rs"]
mod queries;
#[path = "recall_bench/rankers.rs"]
mod rankers;
#[path = "recall_bench/report.rs"]
mod report;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aghist::search::SearchIndex;

use corpus::{build_corpus, TOPICS};
use metrics::{
    percentile_duration, rank_of, BenchTimings, StrategyReport, MAX_INDEX_BUILD_TIME,
    MAX_QUERY_P95, MIN_HYBRID_MRR, MIN_HYBRID_RECALL, MIN_LEXICAL_RECALL,
};
use queries::{load_queries, LabeledQuery};
use rankers::{rank_hybrid, rank_lexical, SemanticRanker};
use report::render_report;

struct QueryScores {
    lexical: StrategyReport,
    semantic: StrategyReport,
    hybrid: StrategyReport,
    latencies: Vec<Duration>,
}

#[test]
fn search_recall_benchmark() {
    let corpus = build_corpus();
    assert_eq!(
        corpus.sessions.len(),
        TOPICS.len() + corpus.noise_sessions,
        "bench corpus session accounting drifted"
    );

    let index_dir = tempfile::tempdir().expect("tempdir for index");
    let index = SearchIndex::open_or_create(index_dir.path()).expect("open index");
    let (tx, _rx) = crossbeam_channel::unbounded();
    let build_started = Instant::now();
    index
        .build_index(&corpus.sessions, &corpus.providers, &tx)
        .expect("build index");
    let index_build = build_started.elapsed();

    let semantic = SemanticRanker::build(&corpus.semantic_texts_with_topic_bodies());
    let queries = load_queries();
    validate_queries(&queries);

    let scores = score_queries(&index, &semantic, &queries);
    let reports = vec![
        scores.lexical.clone(),
        scores.semantic.clone(),
        scores.hybrid.clone(),
    ];
    let timings = BenchTimings {
        index_build,
        query_p95: percentile_duration(scores.latencies, 95),
    };
    let report = render_report(
        &reports,
        queries.len(),
        corpus.sessions.len(),
        corpus.noise_sessions,
        &timings,
    );
    eprintln!("\n{report}");
    write_report_if_requested(&report);
    assert_bench_gates(&scores.lexical, &scores.hybrid, &timings, queries.len());
}

fn validate_queries(queries: &[LabeledQuery]) {
    assert!(
        !queries.is_empty(),
        "labeled query file must contain at least one query"
    );

    let labeled_targets: HashSet<&str> = queries
        .iter()
        .map(|q| q.expected_session_id.as_str())
        .collect();
    for target in &labeled_targets {
        assert!(
            TOPICS.iter().any(|t| &t.id == target),
            "labeled query targets unknown session id: {target}"
        );
    }
    let mut seen_query_ids = HashSet::new();
    for q in queries {
        assert!(
            seen_query_ids.insert(q.id.as_str()),
            "duplicate labeled query id: {}",
            q.id
        );
    }
}

fn score_queries(
    index: &SearchIndex,
    semantic: &SemanticRanker,
    queries: &[LabeledQuery],
) -> QueryScores {
    let mut lexical_report = StrategyReport {
        strategy: "lexical".into(),
        ..Default::default()
    };
    let mut semantic_report = StrategyReport {
        strategy: "semantic".into(),
        ..Default::default()
    };
    let mut hybrid_report = StrategyReport {
        strategy: "hybrid".into(),
        ..Default::default()
    };

    let mut query_latencies = Vec::with_capacity(queries.len());
    for q in queries {
        let query_started = Instant::now();
        let lex_hits = rank_lexical(index, &q.query);
        let sem_hits = semantic.rank(&q.query);
        let hyb_hits = rank_hybrid(&lex_hits, &sem_hits);
        query_latencies.push(query_started.elapsed());

        let lex_rank = rank_of(&lex_hits, &q.expected_session_id);
        let sem_rank = rank_of(&sem_hits, &q.expected_session_id);
        let hyb_rank = rank_of(&hyb_hits, &q.expected_session_id);

        lexical_report.overall.record(lex_rank);
        semantic_report.overall.record(sem_rank);
        hybrid_report.overall.record(hyb_rank);

        lexical_report
            .per_tag
            .entry(q.tag.clone())
            .or_default()
            .record(lex_rank);
        semantic_report
            .per_tag
            .entry(q.tag.clone())
            .or_default()
            .record(sem_rank);
        hybrid_report
            .per_tag
            .entry(q.tag.clone())
            .or_default()
            .record(hyb_rank);

        eprintln!(
            "  {:<3} [{:<8}] {:<40} lex={:?} sem={:?} hyb={:?}",
            q.id, q.tag, q.query, lex_rank, sem_rank, hyb_rank,
        );
    }

    QueryScores {
        lexical: lexical_report,
        semantic: semantic_report,
        hybrid: hybrid_report,
        latencies: query_latencies,
    }
}

fn write_report_if_requested(report: &str) {
    if std::env::var_os("AGHIST_BENCH_WRITE_REPORT").is_some() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/SEARCH_BENCH.md");
        fs::write(&path, report).expect("write SEARCH_BENCH.md");
        eprintln!("wrote {}", path.display());
    }
}

fn assert_bench_gates(
    lexical_report: &StrategyReport,
    hybrid_report: &StrategyReport,
    timings: &BenchTimings,
    query_count: usize,
) {
    assert!(
        lexical_report.overall.recall() >= MIN_LEXICAL_RECALL,
        "lexical recall@10 collapsed: got {:.3}, expected at least {:.3} on {} queries",
        lexical_report.overall.recall(),
        MIN_LEXICAL_RECALL,
        query_count
    );
    assert!(
        hybrid_report.overall.recall() >= MIN_HYBRID_RECALL,
        "hybrid recall@10 collapsed: got {:.3}, expected at least {:.3} on {} queries",
        hybrid_report.overall.recall(),
        MIN_HYBRID_RECALL,
        query_count
    );
    assert!(
        hybrid_report.overall.mrr() >= MIN_HYBRID_MRR,
        "hybrid MRR collapsed: got {:.3}, expected at least {:.3} on {} queries",
        hybrid_report.overall.mrr(),
        MIN_HYBRID_MRR,
        query_count
    );
    assert!(
        timings.index_build <= MAX_INDEX_BUILD_TIME,
        "index build took {:?}, expected <= {:?}",
        timings.index_build,
        MAX_INDEX_BUILD_TIME
    );
    assert!(
        timings.query_p95 <= MAX_QUERY_P95,
        "query p95 took {:?}, expected <= {:?}",
        timings.query_p95,
        MAX_QUERY_P95
    );
}
