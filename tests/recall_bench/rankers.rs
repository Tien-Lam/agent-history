use std::collections::{HashMap, HashSet};

use aghist::search::SearchIndex;

pub(crate) const TOP_K: usize = 10;
const RRF_K: f32 = 60.0;

/// One ranked hit returned by a strategy. `session_id` is what we score
/// against the labeled answer; `score` is opaque (only the rank matters for
/// recall/MRR).
#[derive(Debug, Clone)]
pub(crate) struct RankedHit {
    pub(crate) session_id: String,
    score: f32,
}

/// Run lexical (Tantivy BM25): over-fetch, then collapse message-level hits
/// down to unique session ids in score order. We keep the first occurrence
/// because Tantivy returns hits sorted by score descending.
///
/// Tantivy's `QueryParser` treats characters like `'`, `:`, `+` as syntax, so
/// we sanitize to alphanumeric-plus-spaces before parsing. A parser error here
/// would measure benchmark brittleness rather than recall.
pub(crate) fn rank_lexical(index: &SearchIndex, query: &str) -> Vec<RankedHit> {
    let cleaned = sanitize_lexical(query);
    let raw = index
        .search(&cleaned, 200)
        .expect("lexical search should not error after sanitization");

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hit in raw {
        if seen.insert(hit.session_id.clone()) {
            out.push(RankedHit {
                session_id: hit.session_id,
                score: hit.score,
            });
        }
    }
    out
}

/// Hashed bag-of-words embedding. Deterministic, no model download. Each token
/// contributes to a fixed-dim vector via FNV-1a-mod-DIM, weighted by inverse
/// document frequency. The result correlates with TF-IDF cosine similarity.
pub(crate) struct SemanticRanker {
    /// Map from session id to its document vector.
    docs: HashMap<String, Vec<f32>>,
    /// Inverse document frequency per token bucket.
    idf: Vec<f32>,
    dim: usize,
}

impl SemanticRanker {
    const DIM: usize = 256;

    pub(crate) fn build(session_texts: &[(String, String)]) -> Self {
        let dim = Self::DIM;
        let n_docs = session_texts.len() as f32;

        // Document frequency per bucket: how many docs hit at least once.
        let mut df = vec![0u32; dim];
        let mut per_doc_buckets: Vec<HashSet<usize>> = Vec::with_capacity(session_texts.len());
        for (_, text) in session_texts {
            let mut buckets = HashSet::new();
            for tok in tokenize(text) {
                buckets.insert(bucket(&tok, dim));
            }
            for &b in &buckets {
                df[b] += 1;
            }
            per_doc_buckets.push(buckets);
        }

        let idf: Vec<f32> = df
            .iter()
            .map(|&d| {
                if d == 0 {
                    0.0
                } else {
                    // Smoothed IDF: ln((N + 1) / (df + 1)) + 1.
                    ((n_docs + 1.0) / (d as f32 + 1.0)).ln() + 1.0
                }
            })
            .collect();

        let mut docs = HashMap::new();
        for (id, text) in session_texts {
            let mut vec = vec![0.0f32; dim];
            for tok in tokenize(text) {
                let b = bucket(&tok, dim);
                vec[b] += 1.0;
            }
            for (i, v) in vec.iter_mut().enumerate() {
                *v *= idf[i];
            }
            normalize(&mut vec);
            docs.insert(id.clone(), vec);
        }

        Self { docs, idf, dim }
    }

    pub(crate) fn rank(&self, query: &str) -> Vec<RankedHit> {
        let mut q = vec![0.0f32; self.dim];
        for tok in tokenize(query) {
            let b = bucket(&tok, self.dim);
            q[b] += 1.0;
        }
        for (i, v) in q.iter_mut().enumerate() {
            *v *= self.idf[i];
        }
        normalize(&mut q);

        let mut scored: Vec<RankedHit> = self
            .docs
            .iter()
            .map(|(id, doc)| RankedHit {
                session_id: id.clone(),
                score: dot(&q, doc),
            })
            .filter(|h| h.score > 0.0)
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        scored
    }
}

/// Reciprocal Rank Fusion. Combines two ranked lists into one score per
/// session: `sum 1 / (k + rank_in_list)`. `k = 60` is the canonical value; it
/// dampens the penalty for being outside the top-10 of either list.
pub(crate) fn rank_hybrid(lexical: &[RankedHit], semantic: &[RankedHit]) -> Vec<RankedHit> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for (rank, hit) in lexical.iter().enumerate() {
        *scores.entry(hit.session_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + (rank + 1) as f32);
    }
    for (rank, hit) in semantic.iter().enumerate() {
        *scores.entry(hit.session_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + (rank + 1) as f32);
    }
    let mut out: Vec<RankedHit> = scores
        .into_iter()
        .map(|(id, s)| RankedHit {
            session_id: id,
            score: s,
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    out
}

/// Lowercase, split on non-alphanumeric, drop short tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Replace tantivy-special characters with spaces. Lossier than the real
/// production sanitizer would need to be, but good enough for the bench: every
/// labeled query still keeps its content tokens.
fn sanitize_lexical(query: &str) -> String {
    query
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect()
}

/// FNV-1a -> bucket. Stable across runs/platforms.
fn bucket(token: &str, dim: usize) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in token.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h as usize) % dim
}

fn normalize(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
