# aghist

Cross-platform TUI for viewing and searching AI agent conversation history.
Supports Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode, Cursor, Aider, Zed AI, Cline, and Continue.dev.

## Build & Test

```bash
cargo build                          # dev build
cargo build --features embeddings    # include semantic search (pulls ONNX runtime ~90 MB)
cargo test                           # all tests
cargo test <name>                    # single test by name
cargo insta review                   # review snapshot changes (run after tests that update snapshots)
cargo clippy                         # lint (pedantic enabled)
cargo run                            # launch TUI
cargo run -- --list                  # list sessions without TUI
cargo run -- export -f md -s <id>    # export session to stdout
cargo run -- search <q> --debug-search  # BM25 explanation per hit
cargo run -- mcp                     # JSON-RPC stdio MCP server
cargo run -- schema --list           # list subcommands with JSON-Schemas
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_API_KEY` | Required for `--llm` flag on `decisions`, `todos`, `threads`, and `track` |
| `AGHIST_LLM_MODEL` | Override LLM model (default: `claude-haiku-4-5-20251001`) |
| `AGHIST_LLM_ENDPOINT` | Override API endpoint (default: `api.anthropic.com`) |
| `AGHIST_LLM_API_KEY` | Alias for `ANTHROPIC_API_KEY` |
| `AGHIST_HOME` | Override home dir for tests (provider detection, config loading) |

Config file: `~/.config/aghist/config.toml` (Linux/macOS) — use `aghist health` to verify setup.

## Agent-friendly CLI surface

Every subcommand has a stable error envelope (single-line JSON on stderr), semantic exit codes (`0` success, `1` runtime error, `2` usage error, `3` success-but-empty), JSON output on a pipe (or with `--json`), and a discoverable schema (`aghist schema <subcmd>`). New surfaces beyond the baseline TUI/--list/export:

- `search <query> [--hybrid-weight 0..1] [--debug-search] [--watch] [--query-file F | --stdin | --params JSON]`
- `show <provider>/<session-id>#<turn> [--include-context N]` — citation-ref resolver
- `index [--force] [--accept-download]` — idempotent, delta-aware via content-hash manifest; `--accept-download` enables FastEmbed (~90 MB)
- `health` — machine-readable doctor
- `sources [add | list | remove | pull]` — local provider listing + remote source registry (rsync), federated search across `~/.cache/aghist/sources/<name>/data/`
- `decisions` / `todos` / `threads` — heuristic cross-session extractors (add `--llm` for structured output via Claude)
- `diff <session1> <session2>` — LCS-based turn-by-turn session comparison
- `track <topic>` — LLM-powered cross-session topic change tracker (requires `ANTHROPIC_API_KEY`)
- `schema [--list | --all | <subcmd>]` — JSON-Schema (draft-2020-12) introspection
- `mcp` — JSON-RPC 2.0 stdio server: tools `search_sessions`, `list_sessions`, `get_session`, `get_message`, `reindex`, `health`; resources `aghist://session/<provider>/<id>` and `aghist://session/<provider>/<id>/turn/<n>`

Global filter flags accepted on most subcommands: `--provider <slug>`, `--since/--until <RFC3339>`, `--project <substr>`, `--role user|assistant|tool`, `--has-tool-call`. Global pagination on `--list`: `--limit <N>`, `--cursor <opaque>` (response includes `meta.next_cursor`).

## CLI error envelope and exit codes

Stable `kind` values on stderr as single-line JSON: `{kind, message, hint?}`. Exit codes: `0` success, `1` runtime error, `2` usage error, `3` success-but-empty.

## Code Map

- `src/model/` — Unified types: `Provider`, `Session`, `Message`, `ContentBlock`, newtype IDs
- `src/provider/` — `HistoryProvider` trait (Send + Sync) with per-tool implementations
- `src/app.rs` — TEA architecture: `App`, `AppMode`, `Action` enum for all state transitions
- `src/action.rs` — `Action` enum (every possible state transition)
- `src/event.rs` — Crossterm key events mapped to `Action`s per mode
- `src/ui/` — Ratatui components: `session_list`, `message_view`, `status_bar`
- `src/search.rs` — Tantivy full-text index, incremental rebuild
- `src/export.rs` — Markdown, JSON, HTML export
- `src/config.rs` — TOML config loading
- `src/lib.rs` — Library surface re-exported for integration tests
- `src/main.rs` — Clap CLI, command dispatch, all command implementations (~6k lines)
- `tests/` — Integration and E2E tests with fixture data in `tests/fixtures/`

For detailed architecture, see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Conventions

- No `unsafe` (enforced by lint)
- `clippy::pedantic` enabled
- `thiserror` for library errors, `anyhow` only at binary boundary
- Corrupt/missing session files are skipped, never crash
