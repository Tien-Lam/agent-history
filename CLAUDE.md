# aghist

Cross-platform TUI for viewing and searching AI agent conversation history (Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode).

This project is **bd + gt driven**: tasks live in beads (`bd`), and Gas Town (`gt`) dispatches polecat agents to work them. The full workflow rules are in [`AGENTS.md`](AGENTS.md) — read that before doing any work.

## Build & Test

```bash
cargo build                        # dev build
cargo test                         # all tests
cargo test <name>                  # single test by name
cargo insta review                 # review snapshot changes
cargo clippy                       # lint (pedantic enabled)
cargo run                          # launch TUI
cargo run -- --list                # list sessions without TUI
cargo run -- export -f md -s <id>  # export session to stdout
```

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
- `src/main.rs` — Clap CLI, terminal setup/teardown
- `tests/` — Integration and E2E tests with fixture data in `tests/fixtures/`

For detailed architecture, see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Conventions

- No `unsafe` (enforced by lint)
- `clippy::pedantic` enabled
- `thiserror` for library errors, `anyhow` only at binary boundary
- Corrupt/missing session files are skipped, never crash

## Tooling

- **bd** (beads) — issue tracker. Prefix: `ahist`. Database mode: dolt server, port pinned via `.beads/config.yaml`.
- **gt** (Gas Town) — multi-agent orchestrator. HQ at `~/gt/`, this project registered as rig `aghist`.
- **TodoWrite / TaskCreate are forbidden** — use `bd` for all task tracking. See `AGENTS.md`.
