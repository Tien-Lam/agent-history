# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.1] - 2026-05-24

Maintenance release for the 0.3 line.

### Fixed

- Preserve TUI message-level filters during render, so role/tool/message filters cannot leak messages from hidden sessions.
- Reject unknown provider slugs in config instead of silently ignoring invalid allowlist entries.
- Surface partial provider/source discovery failures consistently in CLI, TUI, and MCP flows.
- Harden provider parsers against recent shape drift across Claude Code, Codex CLI, Copilot CLI, Gemini CLI, OpenCode, Cursor, Cline, Continue.dev, and Zed AI histories.
- Keep source-qualified refs intact across `show`, `export`, `diff`, metadata filters, MCP resources, and federated search results.
- Tighten self-managed `update`/`uninstall` guardrails so build-tree, Cargo, and package-manager binaries are rejected before mutating files.

### Changed

- Split large command, provider, schema, search, TUI, MCP, export, metadata, and report modules into smaller internal modules without changing the public CLI contracts.
- Release archives now include and smoke-test the `aghist.install` marker used by `aghist update` and `aghist uninstall`.
- Trimmed stale install documentation and clarified source installs with `cargo install --locked`.

### Tests

- Added broad provider conformance, JSON/MCP contract, markdown link, supply-chain, release archive, all-features embeddings, recall benchmark, and user-data safety coverage.
- CI now includes supply-chain policy checks, actionlint, release dry-runs, Windows build/test coverage, and all-features clippy/test coverage.

## [0.3.0] - 2026-05-09

The y3o roadmap: aghist grows an agent-friendly CLI surface, semantic search, an MCP server, and cross-machine federated history.

### Added

#### Agent-friendly CLI surface

- `aghist search <query>` — Tantivy BM25 search subcommand (was previously TUI-only)
- `aghist show <provider>/<session-id>#<turn>` — citation-ref resolver, `--include-context N` for surrounding turns
- `aghist health` — machine-readable doctor (`{checks, ok, summary}` envelope)
- `aghist index` — idempotent, delta-aware index management; `--force` for full rebuild; `--accept-download` to opt into FastEmbed
- `aghist sources` — list local provider paths + sizes + last-indexed-at; subcommands `add/list/remove/pull` for cross-machine remotes
- `aghist decisions` / `aghist todos` / `aghist threads` — heuristic cross-session extractors (decisions, follow-ups, project clusters)
- `aghist schema [<subcmd>|--list|--all]` — JSON-Schema (draft-2020-12) introspection for every subcommand
- `aghist mcp` — stdio MCP JSON-RPC 2.0 server: tools `search_sessions`, `list_sessions`, `get_session`, `get_message`, `reindex`, `health`; resources `aghist://session/<provider>/<id>` and `aghist://session/<provider>/<id>/turn/<n>`

#### Output discipline

- Stable JSON error envelope: `{"error":{"kind":"<kebab>","message":"…","hint":"…"}}` on stderr
- Semantic exit codes: `0` success, `1` runtime error, `2` usage error, `3` success-but-empty
- `--json` / `--ndjson` global flags; auto-detect JSON when stdout is not a TTY
- `--params <JSON>` on every subcommand: submit the whole request body in one JSON, validated against `aghist schema <subcmd>`
- `--query-file` and `--stdin` for `aghist search` queries with special characters
- `--watch` mode on `aghist search`: stream NDJSON as new sessions land

#### Filters and pagination

- Global filter flags: `--provider <slug>`, `--since/--until <RFC3339>`, `--project <substr>`, `--role user|assistant|tool`, `--has-tool-call`
- `--limit <N>` + `--cursor <opaque>` opaque-cursor pagination on `--list` and `search`; response includes `meta.next_cursor` and `meta.total`
- `--turn-range A:B` (1-based inclusive) on `aghist export` to slice sessions

#### Citation refs

- Stable `<provider-slug>/<session-id>#<turn>` ref format, opaque-stable across reindex (`src/model/citation.rs`)

#### Search engine

- Tool-call output text indexed in a separate Tantivy field — `--has-tool-call` filter and search hits land on Bash stdout, file reads, grep results, etc.
- `--debug-search` emits per-hit BM25 explanation tree
- `--hybrid-weight <0..1>` opt-in RRF blend of BM25 + FastEmbed cosine ranks (lexical-only at `0`, semantic-only at `1`); fails open to lexical when no consent
- FastEmbed (`AllMiniLML6V2`, ~90 MB) integrated behind `aghist index --accept-download`; consent persisted next to the index
- Embedding cache invalidation keyed by message content hash; reindex of unchanged messages is free
- Recall@10 / MRR bench harness across lexical, semantic, and hybrid (`tests/recall_bench.rs`, `tests/fixtures/bench_recall/`)

#### Cross-machine sources

- `aghist sources add <name> --host <h> --path <p> [--transport ssh|rsync]` — register a remote in `~/.config/aghist/config.toml`
- `aghist sources pull [<name>|--all] [--dry-run]` — rsync remote → `~/.cache/aghist/sources/<name>/data/` (override cache root via `AGHIST_SOURCES_CACHE_DIR`; override rsync binary via `AGHIST_RSYNC_BIN`)
- Federated search: indexer treats every cache as an additional provider source; `aghist search` returns hits from local + remote with a `source` field

#### MCP

- Per-provider allowlist: `[providers] mcp_exposed = [...]` narrows what `aghist mcp` exposes independently of the CLI's `enabled` list (e.g. hide a personal account from work agents)

### Changed

- **Breaking** (JSON only): `Provider` now serializes as kebab-case slug (`"claude-code"`, `"copilot-cli"`, etc.) to match the CLI's `--provider` input. Was snake_case (`"claude_code"`). Affects every JSON response with a `provider` field.
- **Breaking** (JSON only): `aghist decisions --json` now returns `{decisions: [...], count: N}` to match `todos` and `threads`. Was a bare array.
- Index manifest now keyed by content hash instead of mtime — surviving file copies / pulls across machines without spurious re-indexing.

### Fixed

- Provider tests for `copilot_v2_discover_sessions` / `opencode_v2_discover_sessions` now pass on Linux (Windows-style fixture paths previously failed `project_name` extraction).
- Federated search: when local and a remote source contain the same session, `source` label correctly preserves `local` instead of clobbering to the remote (defeating the local-fast-path).

## [0.2.1] - 2026-04-12

### Fixed

- **Security**: Escape HTML language attribute in code block export to prevent XSS injection
- **Security**: Shell-escape session IDs in resume commands to prevent command injection via clipboard
- **Security**: Sanitize export filenames to prevent path traversal via crafted session IDs
- **Safety**: Install panic hook to restore terminal on crash (raw mode + alternate screen)
- **Safety**: Use UTF-8-safe string slicing for session ID truncation and Codex UUID extraction
- Help toggle now preserves previous mode (was always returning to Browse from ViewSession)
- Clear stale search results when session list is reloaded
- Warn on malformed `config.toml` instead of silently falling back to defaults
- Log warning on Windows uninstall cleanup failure instead of ignoring

### Changed

- Migrated from deprecated `serde_yaml` to `serde_yaml_ng` 0.10
- Upgraded `ratatui` 0.29 → 0.30 and `tantivy` 0.22 → 0.26
- Bumped `lru` to 0.16.3, eliminating vulnerable transitive 0.12.5

### Added

- Tests for HTML attribute injection, Unicode/CJK/RTL export, IO error paths, help mode preservation, and export-while-filtered workflow

## [0.2.0] - 2026-04-12

### Added

- Install script (`install.sh`) for one-line installation on Linux and macOS
- `aghist update` subcommand for self-updating from GitHub releases
- `aghist uninstall` subcommand to remove binary, search index, and config
- `cargo binstall` support via package metadata
- Architecture documentation (`docs/ARCHITECTURE.md`)
- LICENSE file (MIT)
- CHANGELOG.md

## [0.1.3] - 2026-04-11

### Changed

- Migrated tests to shared helpers, added search and export workflow tests
- Strengthened tests to assert real behavior instead of just "didn't crash"
- Removed mocked dispatch calls in favor of exercising the real event loop

## [0.1.2] - 2026-04-10

### Added

- Resume command with clipboard copy for all providers (`y` keybinding)
- Cross-platform release workflow (Linux x86_64, Windows x86_64, macOS aarch64)
- Demo tape for recording GIF with VHS

### Fixed

- Windows key handling and UI rendering
- Filter editing, project name display, config loading, and assorted UX issues

## [0.1.0] - 2026-04-10

### Added

- TUI with session browsing, message viewing, and vim-style keybindings
- Five provider parsers: Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode
- Full-text search powered by Tantivy with incremental indexing
- Filter panel (provider, project, date range)
- Export to Markdown, JSON, and HTML
- Configuration via TOML (`~/.config/aghist/config.toml`)
- LRU message cache
- GitHub Actions CI (clippy, tests, build)
- Snapshot tests with insta

[0.3.1]: https://github.com/Tien-Lam/agent-history/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Tien-Lam/agent-history/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Tien-Lam/agent-history/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Tien-Lam/agent-history/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/Tien-Lam/agent-history/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Tien-Lam/agent-history/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/Tien-Lam/agent-history/releases/tag/v0.1.0
