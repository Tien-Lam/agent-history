use super::*;

#[test]
fn cosine_similarity_handles_identical_orthogonal_and_zero_vectors() {
    let a = [1.0_f32, 0.0, 0.0];
    let b = [1.0_f32, 0.0, 0.0];
    let c = [0.0_f32, 1.0, 0.0];
    let z = [0.0_f32, 0.0, 0.0];

    // Identical → 1.0 (within float tolerance).
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    // Orthogonal → 0.0.
    assert!(cosine_similarity(&a, &c).abs() < 1e-6);
    // Zero norm on either side → exactly 0.0 (the function returns 0.0
    // literally, not via float arithmetic, so equality is safe).
    assert!(cosine_similarity(&a, &z).abs() < 1e-6);
    assert!(cosine_similarity(&z, &z).abs() < 1e-6);
    // Mismatched lengths or empty inputs → exactly 0.0 (early return).
    assert!(cosine_similarity(&a, &[1.0_f32, 0.0]).abs() < 1e-6);
    assert!(cosine_similarity(&[][..], &[][..]).abs() < 1e-6);
}

#[test]
fn search_hybrid_falls_back_to_lexical_when_weight_is_zero() {
    let (_dir, index) = build_tiny_index();
    let lex = index
        .search_with_filters("tantivy", 10, &SearchFilters::default())
        .unwrap();
    let hybrid = index
        .search_hybrid(
            "tantivy",
            &[SemanticCandidate {
                message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
                message_id: "m-4".to_string(),
                similarity: 0.9,
            }],
            10,
            &SearchFilters::default(),
            0.0,
            50,
        )
        .unwrap();
    // weight=0 must yield exactly the lexical result, including identical
    // BM25 scores — not RRF scores.
    assert_eq!(hybrid.len(), lex.len());
    for (a, b) in hybrid.iter().zip(lex.iter()) {
        assert_eq!(a.message_id, b.message_id);
        assert!((a.score - b.score).abs() < 1e-6);
    }
}

#[test]
fn search_hybrid_falls_back_to_lexical_when_semantic_pool_is_empty() {
    let (_dir, index) = build_tiny_index();
    let lex = index
        .search_with_filters("tantivy", 10, &SearchFilters::default())
        .unwrap();
    let hybrid = index
        .search_hybrid("tantivy", &[], 10, &SearchFilters::default(), 0.5, 50)
        .unwrap();
    assert_eq!(hybrid.len(), lex.len());
    for (a, b) in hybrid.iter().zip(lex.iter()) {
        assert_eq!(a.message_id, b.message_id);
    }
}

#[test]
fn search_hybrid_promotes_semantic_only_hits() {
    // "tantivy" only matches m-3 lexically. If we pretend the embedding
    // model thinks m-4 ("fastembed semantic vectors") is the top semantic
    // match, RRF should return BOTH — proving hybrid surfaces hits the
    // lexical pass alone wouldn't.
    let (_dir, index) = build_tiny_index();
    let semantic = vec![
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
            message_id: "m-4".to_string(),
            similarity: 0.95,
        },
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(0, "m-3"),
            message_id: "m-3".to_string(),
            similarity: 0.80,
        },
    ];
    let hybrid = index
        .search_hybrid("tantivy", &semantic, 10, &SearchFilters::default(), 0.5, 50)
        .unwrap();
    let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
    assert!(ids.contains(&"m-3"), "lexical hit must survive: {ids:?}");
    assert!(
        ids.contains(&"m-4"),
        "semantic-only hit must surface: {ids:?}"
    );
    // m-3 appears in both pools → its fused score should beat m-4 (which
    // is semantic-only) when weight=0.5.
    let m3_score = hybrid.iter().find(|h| h.message_id == "m-3").unwrap().score;
    let m4_score = hybrid.iter().find(|h| h.message_id == "m-4").unwrap().score;
    assert!(
            m3_score > m4_score,
            "lexical+semantic hit (m-3, score {m3_score}) should outrank semantic-only (m-4, score {m4_score})"
        );
}

#[test]
fn search_hybrid_applies_filters_to_semantic_candidates() {
    // Filter to a project that only contains sess-1, but feed in semantic
    // candidates that include m-4 (in sess-2). Hybrid must drop m-4 — the
    // filter applies symmetrically to both ranking sources.
    let (_dir, index) = build_tiny_index();
    let filters = SearchFilters {
        project: Some("alpha".to_string()),
        ..SearchFilters::default()
    };
    let semantic = vec![
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
            message_id: "m-4".to_string(),
            similarity: 0.95,
        },
        SemanticCandidate {
            message_key: make_session("sess-1", "alpha").message_key(0, "m-1"),
            message_id: "m-1".to_string(),
            similarity: 0.70,
        },
    ];
    let hybrid = index
        .search_hybrid("rust", &semantic, 10, &filters, 0.5, 50)
        .unwrap();
    let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
    assert!(
        !ids.contains(&"m-4"),
        "project filter must drop m-4 from semantic pool: {ids:?}"
    );
    assert!(
        ids.contains(&"m-1"),
        "filter-passing hit must remain: {ids:?}"
    );
}
