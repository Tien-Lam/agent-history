//! Recall@10 / MRR benchmark for the search index.
//!
//! Builds a mixed-provider synthetic corpus, runs a labeled query set against
//! it, and reports recall@10 + MRR + coarse latency gates for each
//! retrieval strategy:
//!
//! - **lexical**: BM25 via Tantivy, exactly what `aghist search` uses today.
//! - **semantic**: cosine similarity on a deterministic hashed bag-of-words
//!   embedding. This is *not* the production embedder (fastembed / MiniLM is
//!   feature-gated and downloads a model on first run); it's a stand-in that
//!   gives us a non-lexical signal we can run anywhere, including CI without
//!   network. The harness can be re-run with the production ranker by swapping
//!   the `SemanticRanker` impl.
//! - **hybrid**: Reciprocal Rank Fusion (RRF) of the lexical and semantic
//!   ranked lists with `k = 60` (the value used in the original RRF paper and
//!   in most public hybrid-search implementations). RRF is the production
//!   hybrid-search fusion method; we bench it here so the fixture can be
//!   reused as a regression gate.
//!
//! ## How to run
//!
//! ```text
//! cargo test --test recall_bench -- --nocapture          # default: prints report
//! AGHIST_BENCH_WRITE_REPORT=1 cargo test --test recall_bench -- --nocapture
//! ```
//!
//! Setting `AGHIST_BENCH_WRITE_REPORT=1` writes an ignored local report to
//! `docs/SEARCH_BENCH.md` with freshly measured numbers. The default test
//! asserts conservative quality and latency floors so the suite stays fast
//! while still catching meaningful regressions without committing stale timing
//! snapshots.

// Pedantic lints we deliberately ignore in this bench:
// - cast_precision_loss / cast_possible_truncation: usize→f32 for
//   recall/MRR arithmetic with values ≪ 2^23, never lossy in practice.
// - format_push_string: writing a markdown table with `push_str(&format!())`
//   is more readable than juggling `writeln!` + import dance for a one-shot.
// - doc_markdown: model names like "MiniLM" don't need backticks in prose.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

mod common;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;
use aghist::search::SearchIndex;

use common::fixtures::{self, ClaudeFixtureBuilder};

// ─── Corpus ─────────────────────────────────────────────────────────────────

/// One synthetic single-topic session. The session id IS the topic slug so the
/// labeled queries can target it directly without a separate lookup table.
struct TopicSpec {
    /// Stable session id; appears in `queries.json` as `expected_session_id`.
    id: &'static str,
    project: &'static str,
    /// First user message — usually a paraphrased question that introduces
    /// the topic terminology.
    user_prompt: &'static str,
    /// Assistant reply, packed with topic-specific terms so the index has
    /// distinctive content to score against.
    assistant_reply: &'static str,
    /// Follow-up exchange to give the index more than two messages per session.
    user_followup: &'static str,
    assistant_followup: &'static str,
}

const TOPICS: &[TopicSpec] = &[
    TopicSpec {
        id: "kubernetes-deploy",
        project: "kube-platform",
        user_prompt: "How do I deploy this app to our Kubernetes cluster?",
        assistant_reply: "Run kubectl apply -f manifests/. The Deployment spec sets replicas to 3, and the kube-scheduler will assign pods to nodes based on resource requests. Check pod status with kubectl get pods -n prod.",
        user_followup: "What about rolling updates?",
        assistant_followup: "Set strategy.type to RollingUpdate in the Deployment; maxSurge and maxUnavailable control how many pods churn during the roll.",
    },
    TopicSpec {
        id: "database-migration",
        project: "billing-svc",
        user_prompt: "We need to add a column to the orders table without downtime.",
        assistant_reply: "Use alembic revision --autogenerate to scaffold the migration, then run alembic upgrade head against the postgres replica first. Keep the column nullable on initial deploy so the old code path still inserts cleanly.",
        user_followup: "How do we roll back if something goes wrong?",
        assistant_followup: "alembic downgrade -1 reverts the last revision; the schema-upgrade pipeline keeps a snapshot of the previous version for fast rollback.",
    },
    TopicSpec {
        id: "async-rust",
        project: "ingest-pipeline",
        user_prompt: "I'm spawning 10k tasks and the runtime is stalling.",
        assistant_reply: "Check that you're not blocking inside an async fn. tokio uses a cooperative scheduler — long-running CPU work needs spawn_blocking. Also raise worker_threads if you have CPU headroom; tokio defaults to one per core.",
        user_followup: "Should I switch to async-std instead?",
        assistant_followup: "Probably not. tokio is the de-facto runtime; await ergonomics are the same and the futures ecosystem is thicker.",
    },
    TopicSpec {
        id: "react-component",
        project: "dashboard-ui",
        user_prompt: "My component re-renders too often when the parent updates state.",
        assistant_reply: "Wrap the child in React.memo and verify the props are stable. useState getters are referentially stable, but useEffect dependency arrays often capture fresh objects on each render — wrap derived values in useMemo.",
        user_followup: "What about prop drilling?",
        assistant_followup: "Lift the state into a context provider once you're passing the same prop more than three levels deep; otherwise context is overkill.",
    },
    TopicSpec {
        id: "docker-build",
        project: "release-tooling",
        user_prompt: "Image is 1.2 GB. How do I shrink it?",
        assistant_reply: "Switch to a multi-stage Dockerfile: build in a fat image, COPY --from=builder the final binary into a distroless or alpine base. Order RUN steps from least- to most-frequently-changed so layer caching keeps subsequent rebuilds fast.",
        user_followup: "Where do I push the trimmed image?",
        assistant_followup: "Tag it for the internal registry and push during CI; the registry GC reaps untagged layers after 30 days.",
    },
    TopicSpec {
        id: "ci-pipeline",
        project: "release-tooling",
        user_prompt: "I want to run unit tests on every PR.",
        assistant_reply: "Add a GitHub Actions workflow under .github/workflows/test.yml. Use a matrix over OS x toolchain so the green check covers ubuntu and macOS. Cache cargo registry and target/ to keep the test job under 5 minutes.",
        user_followup: "How do I gate merges on the green check?",
        assistant_followup: "Set the required status check in branch protection; the merge button stays disabled until the workflow reports success.",
    },
    TopicSpec {
        id: "memory-leak",
        project: "inference-server",
        user_prompt: "RSS keeps climbing during long-running inference jobs.",
        assistant_reply: "Run the binary under valgrind --leak-check=full on a small workload first. For Python interop, the garbage collector won't reclaim cycles that involve C extensions — break refs explicitly. A heap profile from jemalloc shows where bytes are pinned.",
        user_followup: "Any tools for production where valgrind is too slow?",
        assistant_followup: "heaptrack or pprof have lower overhead and can attach to a running process; sample mode keeps the slowdown under 5%.",
    },
    TopicSpec {
        id: "authn-token",
        project: "auth-svc",
        user_prompt: "Should access tokens live for an hour or a day?",
        assistant_reply: "Short JWT lifetimes (10-15 min) plus a long-lived refresh token is the standard pattern. Sign the JWT with RS256 so verifiers don't need the private key, and rotate the refresh token on every use to invalidate stolen ones.",
        user_followup: "What about OAuth integration?",
        assistant_followup: "Use the authorization code flow with PKCE for the SPA; the device flow only makes sense for CLI logins on browserless boxes.",
    },
    TopicSpec {
        id: "tls-cert",
        project: "edge-gateway",
        user_prompt: "We're getting cert expiry warnings on the gateway.",
        assistant_reply: "Switch to Let's Encrypt with the ACME HTTP-01 challenge. certbot --webroot will renew x509 certificates automatically; a daily systemd timer runs the renewal hook. Make sure the cert chain includes the intermediate or older clients reject it.",
        user_followup: "Can we use DNS-01 instead?",
        assistant_followup: "Yes — DNS-01 lets you issue wildcard certificates and works behind firewalls; you'll need an API token for the DNS provider so certbot can write the validation records.",
    },
    TopicSpec {
        id: "typescript-types",
        project: "dashboard-ui",
        user_prompt: "I have a function that takes either a string or an object and the types are blowing up.",
        assistant_reply: "Use a discriminated union and narrow with a tag field. Conditional types (T extends X ? A : B) help when the shape changes per generic param. typeof guards collapse the union for the compiler.",
        user_followup: "How do I make the generics inferable from usage?",
        assistant_followup: "Constrain generics with `extends` rather than defaulting; type narrowing flows through the constraint and the inference engine picks the right branch.",
    },
    TopicSpec {
        id: "graphql-schema",
        project: "api-gateway",
        user_prompt: "Adding a new field broke our existing clients.",
        assistant_reply: "GraphQL is supposed to be additive — new fields are safe. The break is probably in a resolver returning the old shape. Use Apollo's @deprecated directive on retired fields and keep the resolver alive for one release. Mutations need explicit input types so renames don't silently drop arguments.",
        user_followup: "How do I push real-time updates?",
        assistant_followup: "Subscriptions over WebSocket; use a pubsub backend like Redis to fan out events to subscribers across instances.",
    },
    TopicSpec {
        id: "linux-perf",
        project: "ingest-pipeline",
        user_prompt: "Throughput dropped 30% after the last release.",
        assistant_reply: "perf top -p <pid> shows hot functions in real time. Generate a flame graph with perf record + stackcollapse to see the call-tree. ftrace catches scheduling stalls; bpftrace is great for syscall latency histograms.",
        user_followup: "What about lock contention?",
        assistant_followup: "perf lock or `mutrace` surface the worst offenders; if it's a Rust binary, parking_lot's deadlock detection helps in dev.",
    },
    TopicSpec {
        id: "mcp-server",
        project: "agent-tools",
        user_prompt: "I need an MCP server that exposes search tools over stdio.",
        assistant_reply: "Implement JSON-RPC 2.0 over stdin/stdout and advertise tools/list plus resources/list. The MCP client calls tools/call with structured params; keep responses as content blocks with text and optional structuredContent.",
        user_followup: "How should sessions be exposed as resources?",
        assistant_followup: "Use stable aghist://session/<provider>/<id> URIs and a turn URI for individual messages. Resource reads should be read-only and deterministic.",
    },
    TopicSpec {
        id: "release-install",
        project: "release-tooling",
        user_prompt: "The installer needs to pull GitHub release archives safely.",
        assistant_reply: "Package tar.gz artifacts on Unix and zip artifacts on Windows. Include an aghist.install marker with method=github-release, repo, target, and tag so update and uninstall can verify ownership.",
        user_followup: "How do we test that before tagging?",
        assistant_followup: "Run a release dry-run in CI: build the self-update binary, package a synthetic tag, install from the local archive, verify --version, then uninstall non-interactively.",
    },
    TopicSpec {
        id: "sqlite-metadata",
        project: "metadata",
        user_prompt: "Where should user notes and tags live?",
        assistant_reply: "Use a rusqlite sidecar database with migrations. Store stars, notes, and tags keyed by the stable session ref; keep metadata separate from provider history so aghist remains read-only against source files.",
        user_followup: "What about schema upgrades?",
        assistant_followup: "Run rusqlite_migration at open time and keep migrations additive. Tests should cover CRUD plus filtering by metadata-backed session keys.",
    },
    TopicSpec {
        id: "ratatui-tui",
        project: "terminal-ui",
        user_prompt: "The terminal UI flickers when I scroll message history.",
        assistant_reply: "Keep ratatui rendering pure from App state and mutate only through Action dispatch. Crossterm raw mode setup and teardown belongs at the runtime boundary; MessageView should own scroll offsets and wrapping.",
        user_followup: "How do key bindings fit?",
        assistant_followup: "Map KeyEvent to Action per AppMode, then let dispatch update state before render. Snapshot tests catch layout drift in browse, search, help, and filter overlays.",
    },
    TopicSpec {
        id: "tantivy-index",
        project: "search",
        user_prompt: "Reindexing all sessions every run is wasteful.",
        assistant_reply: "Keep a manifest of source file fingerprints and session keys. Tantivy can skip unchanged sessions, prune stale documents, and keep BM25 fields for provider, project, role, content, tool output, and timestamp.",
        user_followup: "How do duplicate session IDs behave?",
        assistant_followup: "Use provider plus source path plus session id as the internal identity key so duplicate raw IDs from different files do not overwrite each other.",
    },
    TopicSpec {
        id: "embeddings-hybrid",
        project: "search",
        user_prompt: "Lexical search misses paraphrases in old conversations.",
        assistant_reply: "Add optional FastEmbed vectors behind explicit download consent. Store embeddings by message content hash and combine semantic cosine ranking with BM25 using Reciprocal Rank Fusion.",
        user_followup: "What if embeddings are not installed?",
        assistant_followup: "Fail open to lexical search and report the engine in response metadata. Hybrid weights should never require a model download unless the user opted in.",
    },
    TopicSpec {
        id: "federated-sources",
        project: "sync",
        user_prompt: "I want to search agent history from my laptop and workstation together.",
        assistant_reply: "Register remote sources with host, path, and transport, then pull via rsync into a local cache. The indexer treats cached roots as provider candidate dirs and labels hits with the source name.",
        user_followup: "How do local and remote duplicates work?",
        assistant_followup: "Deduplicate by content hash and prefer the local source label when the same session appears in both places.",
    },
    TopicSpec {
        id: "rust-error-handling",
        project: "cli-contracts",
        user_prompt: "The CLI needs machine-readable errors for scripts.",
        assistant_reply: "Use thiserror for domain errors and convert failures to a single-line ErrorEnvelope on stderr. Keep stdout clean for JSON payloads and return semantic exit codes for success, usage errors, runtime errors, and empty results.",
        user_followup: "Where should anyhow be used?",
        assistant_followup: "Keep anyhow at the binary boundary where context is useful; library modules should expose typed errors so callers can classify failures.",
    },
];

fn build_topic_corpus() -> common::fixtures::FixtureDir {
    let mut builder = ClaudeFixtureBuilder::new();
    for topic in TOPICS {
        builder = builder
            .add_session(topic.id)
            .project(topic.project)
            .display(topic.id)
            .user(topic.user_prompt)
            .assistant(topic.assistant_reply)
            .user(topic.user_followup)
            .assistant(topic.assistant_followup)
            .done();
    }
    builder.build()
}

struct BenchCorpus {
    _dirs: Vec<tempfile::TempDir>,
    providers: Vec<Box<dyn HistoryProvider>>,
    sessions: Vec<aghist::model::Session>,
    semantic_texts: Vec<(String, String)>,
    noise_sessions: usize,
}

fn build_corpus() -> BenchCorpus {
    let topic_fixture = build_topic_corpus();
    let mut dirs = vec![topic_fixture.dir];
    let mut providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(ClaudeCodeProvider::new(vec![
            topic_fixture.base_path,
        ]))];

    let (mut noise_dirs, mut noise_providers) = fixtures::all_generated_providers(2, 4);
    dirs.append(&mut noise_dirs);
    providers.append(&mut noise_providers);

    let mut sessions = Vec::new();
    let mut semantic_texts = Vec::new();
    let mut noise_sessions = 0;
    for (provider_idx, provider) in providers.iter().enumerate() {
        let provider_sessions = provider
            .discover_sessions()
            .unwrap_or_else(|e| panic!("discover bench sessions for {}: {e}", provider.provider()));
        if provider_idx > 0 {
            noise_sessions += provider_sessions.len();
        }
        for session in provider_sessions {
            let messages = provider.load_messages(&session).unwrap_or_else(|e| {
                panic!(
                    "load bench messages for {} {}: {e}",
                    provider.provider(),
                    session.id.0
                )
            });
            semantic_texts.push((session.id.0.clone(), render_messages_text(&messages)));
            sessions.push(session);
        }
    }

    BenchCorpus {
        _dirs: dirs,
        providers,
        sessions,
        semantic_texts,
        noise_sessions,
    }
}

// ─── Labeled query set ──────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct LabeledQuery {
    id: String,
    tag: String,
    query: String,
    expected_session_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct QueryFile {
    queries: Vec<LabeledQuery>,
}

fn load_queries() -> Vec<LabeledQuery> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bench_recall/queries.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let file: QueryFile = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    file.queries
}

// ─── Rankers ────────────────────────────────────────────────────────────────

const TOP_K: usize = 10;
const RRF_K: f32 = 60.0;
const MIN_LEXICAL_RECALL: f32 = 0.85;
const MIN_HYBRID_RECALL: f32 = 0.85;
const MIN_HYBRID_MRR: f32 = 0.80;
const MAX_INDEX_BUILD_TIME: Duration = Duration::from_secs(10);
const MAX_QUERY_P95: Duration = Duration::from_millis(750);

/// One ranked hit returned by a strategy. `session_id` is what we score
/// against the labeled answer; `score` is opaque (only the rank matters for
/// recall/MRR).
#[derive(Debug, Clone)]
struct RankedHit {
    session_id: String,
    score: f32,
}

/// Run lexical (Tantivy BM25) — over-fetch, then collapse message-level hits
/// down to unique session ids in score order. We keep the *first* occurrence
/// because Tantivy returns hits sorted by score descending, so the earliest
/// occurrence has the best score.
///
/// Tantivy's `QueryParser` treats characters like `'`, `:`, `+` as syntax,
/// so we sanitize to alphanumeric-plus-spaces before parsing — that's the
/// shape `aghist search` users actually type, and a parser error here would
/// just be measuring our brittleness rather than recall.
fn rank_lexical(index: &SearchIndex, query: &str) -> Vec<RankedHit> {
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

/// Hashed bag-of-words embedding. Deterministic, no model download. Each
/// token contributes to a fixed-dim vector via FNV-1a-mod-DIM, weighted by
/// inverse document frequency. The result correlates with cosine similarity
/// on TF-IDF — *not* a substitute for real semantic embeddings, but it
/// behaves differently from BM25 and gives the bench a meaningful "hybrid"
/// signal when the production embedder isn't wired in yet.
struct SemanticRanker {
    /// Map from session id → its document vector.
    docs: HashMap<String, Vec<f32>>,
    /// Inverse document frequency per token bucket.
    idf: Vec<f32>,
    dim: usize,
}

impl SemanticRanker {
    const DIM: usize = 256;

    fn build(session_texts: &[(String, String)]) -> Self {
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

    fn rank(&self, query: &str) -> Vec<RankedHit> {
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
/// session: `Σ 1 / (k + rank_in_list)`. `k = 60` is the canonical value;
/// it dampens the penalty for being outside the top-10 of either list so
/// items present in both still rank above ones present in only one.
fn rank_hybrid(lexical: &[RankedHit], semantic: &[RankedHit]) -> Vec<RankedHit> {
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

// ─── Metrics ────────────────────────────────────────────────────────────────

/// Rank of `expected` within `hits`, or `None` if not in the top `TOP_K`.
fn rank_of(hits: &[RankedHit], expected: &str) -> Option<usize> {
    hits.iter()
        .take(TOP_K)
        .position(|h| h.session_id == expected)
        .map(|i| i + 1)
}

#[derive(Debug, Default, Clone)]
struct Metrics {
    n: usize,
    hits_at_k: usize,
    reciprocal_rank_sum: f32,
}

impl Metrics {
    fn record(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            self.hits_at_k += 1;
            self.reciprocal_rank_sum += 1.0 / r as f32;
        }
    }

    fn recall(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.hits_at_k as f32 / self.n as f32
        }
    }

    fn mrr(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.reciprocal_rank_sum / self.n as f32
        }
    }
}

/// One row of the per-strategy report: overall + per-tag breakdowns.
#[derive(Debug, Default, Clone)]
struct StrategyReport {
    strategy: String,
    overall: Metrics,
    per_tag: HashMap<String, Metrics>,
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Lowercase, split on non-alphanumeric, drop short tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Replace tantivy-special characters with spaces. Lossier than the real
/// production sanitizer would need to be (we drop apostrophes inside words),
/// but good enough for the bench: every labeled query still keeps its
/// content tokens.
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

fn render_session_text(topic: &TopicSpec) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        topic.user_prompt,
        topic.assistant_reply,
        topic.user_followup,
        topic.assistant_followup,
        topic.id,
    )
}

fn render_messages_text(messages: &[aghist::model::Message]) -> String {
    messages
        .iter()
        .flat_map(|message| message.content.iter())
        .map(content_block_text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn content_block_text(block: &aghist::model::ContentBlock) -> String {
    match block {
        aghist::model::ContentBlock::Text(text)
        | aghist::model::ContentBlock::Thinking(text)
        | aghist::model::ContentBlock::Error(text) => text.clone(),
        aghist::model::ContentBlock::CodeBlock { code, .. } => code.clone(),
        aghist::model::ContentBlock::ToolUse(call) => {
            format!("{} {}", call.name, call.arguments)
        }
        aghist::model::ContentBlock::ToolResult(result) => result.output.clone(),
    }
}

// ─── Report rendering ───────────────────────────────────────────────────────

fn render_report(
    reports: &[StrategyReport],
    n_queries: usize,
    n_sessions: usize,
    noise_sessions: usize,
    timings: &BenchTimings,
) -> String {
    let mut out = String::new();
    out.push_str("# Search Recall Bench\n\n");
    out.push_str("Generated by `cargo test --test recall_bench`. See ");
    out.push_str("[`tests/recall_bench.rs`](../tests/recall_bench.rs) for the harness ");
    out.push_str("and [`tests/fixtures/bench_recall/queries.json`](../tests/fixtures/bench_recall/queries.json) ");
    out.push_str("for the labeled query set.\n\n");
    out.push_str(&format!(
        "Corpus: {n_sessions} synthetic sessions ({topic_count} labeled Claude-format topics + {noise_sessions} mixed-provider distractors), 4 messages each.\n",
        topic_count = TOPICS.len(),
    ));
    out.push_str(&format!("Query set: {n_queries} labeled queries.\n\n"));

    out.push_str("## Overall\n\n");
    out.push_str("| Strategy | Recall@10 | MRR |\n");
    out.push_str("|---|---:|---:|\n");
    for r in reports {
        out.push_str(&format!(
            "| {} | {:.3} | {:.3} |\n",
            r.strategy,
            r.overall.recall(),
            r.overall.mrr(),
        ));
    }

    let mut tags: Vec<String> = reports
        .iter()
        .flat_map(|r| r.per_tag.keys().cloned())
        .collect();
    tags.sort();
    tags.dedup();

    if !tags.is_empty() {
        out.push_str("\n## By query type\n\n");
        out.push_str("| Strategy | Tag | Recall@10 | MRR |\n");
        out.push_str("|---|---|---:|---:|\n");
        for r in reports {
            for tag in &tags {
                if let Some(m) = r.per_tag.get(tag) {
                    out.push_str(&format!(
                        "| {} | {} | {:.3} | {:.3} |\n",
                        r.strategy,
                        tag,
                        m.recall(),
                        m.mrr(),
                    ));
                }
            }
        }
    }

    out.push_str("\n## Latency Gates\n\n");
    out.push_str("| Measurement | Value | Gate |\n");
    out.push_str("|---|---:|---:|\n");
    out.push_str(&format!(
        "| Index build | {} ms | <= {} ms |\n",
        timings.index_build.as_millis(),
        MAX_INDEX_BUILD_TIME.as_millis(),
    ));
    out.push_str(&format!(
        "| Query p95 | {} ms | <= {} ms |\n",
        timings.query_p95.as_millis(),
        MAX_QUERY_P95.as_millis(),
    ));

    out.push_str("\n## Methodology notes\n\n");
    out.push_str("- **lexical** is the production Tantivy BM25 index (`SearchIndex::search`).\n");
    out.push_str(
        "- **semantic** is a deterministic hashed-bag-of-words IDF cosine ranker baked into the ",
    );
    out.push_str(
        "harness — *not* the production fastembed/MiniLM embedder. It exists so the bench ",
    );
    out.push_str(
        "runs in CI without network and so the hybrid row has a non-trivial second signal. ",
    );
    out.push_str("Swap `SemanticRanker` for the production embedder when comparing against real FastEmbed output.\n");
    out.push_str("- **hybrid** = Reciprocal Rank Fusion of the two lists, k = 60.\n");
    out.push_str("- recall@10 = fraction of queries whose target is in the top 10.\n");
    out.push_str(
        "- MRR = mean of 1/rank for each query (rank counted from 1; 0 if outside top 10).\n",
    );
    out.push_str(
        "- Query tags: `lexical` queries have the target's exact terms; `semantic` queries ",
    );
    out.push_str(
        "use paraphrases. The breakdown surfaces where each strategy actually earns its keep.\n",
    );

    out
}

#[derive(Debug, Clone, Copy)]
struct BenchTimings {
    index_build: Duration,
    query_p95: Duration,
}

fn percentile_duration(mut durations: Vec<Duration>, percentile_percent: usize) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    durations.sort_unstable();
    let max_idx = durations.len() - 1;
    let idx = max_idx.saturating_mul(percentile_percent).div_ceil(100);
    durations[idx.min(max_idx)]
}

// ─── Driver ─────────────────────────────────────────────────────────────────

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

    let mut semantic_texts = corpus.semantic_texts.clone();
    for topic in TOPICS {
        if let Some((_, text)) = semantic_texts
            .iter_mut()
            .find(|(session_id, _)| session_id == topic.id)
        {
            *text = render_session_text(topic);
        }
    }
    let semantic = SemanticRanker::build(&semantic_texts);

    let queries = load_queries();
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
    for q in &queries {
        assert!(
            seen_query_ids.insert(q.id.as_str()),
            "duplicate labeled query id: {}",
            q.id
        );
    }

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
    for q in &queries {
        let query_started = Instant::now();
        let lex_hits = rank_lexical(&index, &q.query);
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

        // Eyeball-friendly per-query trace; visible with `--nocapture`.
        eprintln!(
            "  {:<3} [{:<8}] {:<40} lex={:?} sem={:?} hyb={:?}",
            q.id, q.tag, q.query, lex_rank, sem_rank, hyb_rank,
        );
    }

    let reports = vec![
        lexical_report.clone(),
        semantic_report.clone(),
        hybrid_report.clone(),
    ];
    let timings = BenchTimings {
        index_build,
        query_p95: percentile_duration(query_latencies, 95),
    };
    let report = render_report(
        &reports,
        queries.len(),
        corpus.sessions.len(),
        corpus.noise_sessions,
        &timings,
    );
    eprintln!("\n{report}");

    if std::env::var_os("AGHIST_BENCH_WRITE_REPORT").is_some() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/SEARCH_BENCH.md");
        fs::write(&path, &report).expect("write SEARCH_BENCH.md");
        eprintln!("wrote {}", path.display());
    }

    // Quality and latency floors. These are intentionally conservative, but
    // they make search regressions visible in CI instead of burying them in
    // a markdown report.
    assert!(
        lexical_report.overall.recall() >= MIN_LEXICAL_RECALL,
        "lexical recall@10 collapsed: got {:.3}, expected at least {:.3} on {} queries",
        lexical_report.overall.recall(),
        MIN_LEXICAL_RECALL,
        queries.len()
    );
    assert!(
        hybrid_report.overall.recall() >= MIN_HYBRID_RECALL,
        "hybrid recall@10 collapsed: got {:.3}, expected at least {:.3} on {} queries",
        hybrid_report.overall.recall(),
        MIN_HYBRID_RECALL,
        queries.len()
    );
    assert!(
        hybrid_report.overall.mrr() >= MIN_HYBRID_MRR,
        "hybrid MRR collapsed: got {:.3}, expected at least {:.3} on {} queries",
        hybrid_report.overall.mrr(),
        MIN_HYBRID_MRR,
        queries.len()
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
