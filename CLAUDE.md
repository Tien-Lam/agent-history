# aghist

Cross-platform TUI for AI agent conversation history (Claude Code, Copilot CLI,
Gemini, Codex, OpenCode, Cursor, Aider, Zed AI, Cline, Continue.dev).

## Build & Test

```bash
cargo build
cargo build --features embeddings   # opt-in semantic search (ONNX, ~90 MB download)
cargo test
cargo clippy                         # pedantic lints enabled in Cargo.toml
cargo insta review                   # after test failures that update snapshots
```

## Architecture

- `HistoryProvider` trait (`src/provider/mod.rs`) — implement `detect()` + `discover_sessions()` + `load_messages()` to add a provider; register in `detect_all_providers()`
- `App` (`src/app.rs`) — TEA loop: `Action` enum → `dispatch()` → re-render
- `src/main.rs` — all CLI subcommand dispatch and implementations (~6k lines)
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
3. Register in `detect_all_providers()` (`src/provider/mod.rs`)
4. Add color to `provider_color()` (`src/ui/session_list.rs`)
5. Add fixture under `tests/fixtures/<name>/` and unit tests in the provider file
