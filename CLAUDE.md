# aghist

Cross-platform TUI for viewing and searching AI agent conversation history (Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode).

## Build & Test

```bash
cargo build              # dev build
cargo test               # run all tests
cargo clippy             # lint (pedantic enabled)
cargo run                # launch TUI
cargo run -- --list      # list sessions without TUI
```

## Architecture

Rust 2021, lib + bin crate. No async — uses `crossbeam-channel` for concurrency.

### Key modules

- `src/model/` — Unified types: `Provider`, `Session`, `Message`, `ContentBlock`, newtype IDs
- `src/provider/` — `HistoryProvider` trait (Send + Sync) with per-tool implementations
- `src/app.rs` — TEA architecture: `App`, `AppMode`, `Action` enum for all state transitions
- `src/event.rs` — Crossterm key events mapped to `Action`s per mode
- `src/ui/` — Ratatui components: `session_list`, `message_view`, `status_bar`
- `src/main.rs` — Clap CLI, terminal setup/teardown

### Conventions

- No `unsafe` (enforced by lint)
- `clippy::pedantic` enabled
- `thiserror` for library errors, `anyhow` only at binary boundary
- Corrupt/missing session files are skipped, never crash

## Issue Tracking

Uses **beads** (`bd`). Run `bd ready` for available work, `bd prime` for full workflow context.
