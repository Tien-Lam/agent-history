# aghist

Cross-platform TUI for AI agent conversation history (Claude Code, Copilot CLI,
Gemini, Codex, OpenCode, Cursor, Aider, Zed AI, Cline, Continue.dev).

## Build & Test

```bash
cargo build
cargo build --features embeddings   # opt-in semantic search (ONNX, ~90 MB download)
cargo test
cargo clippy
cargo test --test recall_bench -- --nocapture
bash scripts/smoke-release-install.sh --tag v0.0.0-local
cargo insta review                   # after test failures that update snapshots
```

## Architecture

- `HistoryProvider` trait (`src/provider/mod.rs`) — implement `detect()` + `discover_sessions()` + `load_messages()` to add a provider; register runtime construction in `src/provider/registry.rs`
- `App` (`src/app.rs`, `src/app/*`) — TEA loop: `Action` enum → `dispatch()` → re-render
- `src/main.rs` — binary boundary: tracing, clap errors, dispatch entrypoint
- `src/commands/` — CLI command routing and implementations
- `src/session_resolver.rs` — local/remote/source-qualified session and citation lookup
- `src/lib.rs` — re-exports everything for integration tests
- Search index at `~/.cache/aghist/search-index/`; metadata sidecar at `~/.local/share/aghist/metadata.db`

## Conventions

- No `unsafe` (enforced by lint)
- `thiserror` in library code, `anyhow` only at the binary boundary (`main.rs`)
- Corrupt/missing session files must be skipped, never crash
- CLI errors go to stderr as single-line JSON `{kind, message, hint?}`; stdout is clean for piping
- `AGHIST_HOME` overrides the home dir in tests — all provider `detect()` functions respect it

## Adding a provider

1. Add variant to `Provider` enum (`src/model/provider.rs`) — slug, `as_str`, `from_slug`, `all()`, `resume_command`
2. Create `src/provider/<name>.rs` — implement `HistoryProvider`; use `AGHIST_HOME` for testability
3. Register detection/stateless/remote-dir construction in `src/provider/registry.rs`
4. Add color to `provider_color()` (`src/ui/mod.rs`)
5. Add focused provider parser tests and generated fixture support under `tests/common/fixtures/`
6. Run `cargo test --test provider_conformance`; update the provider contract snapshot only when the normalized output intentionally changes
