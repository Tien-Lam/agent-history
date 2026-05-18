use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use crate::common::fixtures;
use crate::common::fixtures::claude::ClaudeFixtureBuilder;

/// One synthetic single-topic session. The session id IS the topic slug so the
/// labeled queries can target it directly without a separate lookup table.
pub(crate) struct TopicSpec {
    /// Stable session id; appears in `queries.json` as `expected_session_id`.
    pub(crate) id: &'static str,
    pub(crate) project: &'static str,
    /// First user message, usually a paraphrased question that introduces
    /// the topic terminology.
    pub(crate) user_prompt: &'static str,
    /// Assistant reply, packed with topic-specific terms so the index has
    /// distinctive content to score against.
    pub(crate) assistant_reply: &'static str,
    /// Follow-up exchange to give the index more than two messages per session.
    pub(crate) user_followup: &'static str,
    pub(crate) assistant_followup: &'static str,
}

pub(crate) const TOPICS: &[TopicSpec] = &[
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

pub(crate) struct BenchCorpus {
    pub(crate) _dirs: Vec<tempfile::TempDir>,
    pub(crate) providers: Vec<Box<dyn HistoryProvider>>,
    pub(crate) sessions: Vec<aghist::model::Session>,
    semantic_texts: Vec<(String, String)>,
    pub(crate) noise_sessions: usize,
}

impl BenchCorpus {
    pub(crate) fn semantic_texts_with_topic_bodies(&self) -> Vec<(String, String)> {
        let mut semantic_texts = self.semantic_texts.clone();
        for topic in TOPICS {
            if let Some((_, text)) = semantic_texts
                .iter_mut()
                .find(|(session_id, _)| session_id == topic.id)
            {
                *text = render_session_text(topic);
            }
        }
        semantic_texts
    }
}

pub(crate) fn build_corpus() -> BenchCorpus {
    let topic_fixture = build_topic_corpus();
    let mut dirs = vec![topic_fixture.dir];
    let mut providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(ClaudeCodeProvider::new(vec![
            topic_fixture.base_path,
        ]))];

    let (mut noise_dirs, mut noise_providers) = fixtures::generated::all_generated_providers(2, 4);
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

fn build_topic_corpus() -> crate::common::fixtures::core::FixtureDir {
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
