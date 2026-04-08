# aghist

Cross-platform TUI for viewing and searching AI agent conversation history (Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode).

## Build & Test

```bash
cargo build              # dev build
cargo test               # run all tests
cargo clippy              # lint (pedantic enabled)
cargo run                 # launch TUI
cargo run -- --list       # list sessions without TUI
```

## Architecture

Rust 2021, single crate. No async — uses `crossbeam-channel` + `rayon` for concurrency.

### Key modules

- `src/model/` — Unified types: `Provider` (Copy enum), `Session`, `Message`, `ContentBlock`, newtype IDs (`SessionId`, `MessageId`)
- `src/provider/` — `HistoryProvider` trait (Send + Sync) with implementations for each tool. Each provider discovers sessions and loads messages from its storage format.
- `src/app.rs` — TEA (The Elm Architecture): `App` struct, `AppMode` enum, synchronous main loop. All state transitions go through `Action` enum.
- `src/action.rs` — Every possible state transition declared as an enum variant
- `src/event.rs` — Crossterm keyboard events mapped to `Action`s per `AppMode`
- `src/ui/` — Ratatui components: `session_list`, `message_view`, `status_bar`
- `src/main.rs` — Clap CLI, terminal setup/teardown

### Provider storage locations

| Provider | Path | Format |
|---|---|---|
| Claude Code | `~/.claude/projects/{project}/{session}.jsonl` | JSONL |
| Copilot CLI | `~/.copilot/session-state/{uuid}/` | JSONL + YAML |
| Gemini CLI | `~/.gemini/tmp/{project}/chats/session-*.json` | JSON |
| Codex CLI | `~/.codex/sessions/{YYYY}/{MM}/{DD}/rollout-*.jsonl` | JSONL |
| OpenCode | `~/.local/share/opencode/storage/` or `%APPDATA%\opencode\` | JSON |

### Error handling

- `thiserror` for typed domain errors (`ProviderError`) in library modules
- `anyhow` only at the binary boundary (`main.rs`)
- Corrupt/missing session files are skipped gracefully, never crash

### Conventions

- No `unsafe` (enforced by lint)
- `clippy::pedantic` enabled
- Conventional commits: `feat:`, `fix:`, `test:`, `chore:`, `refactor:`
- Branch naming: `feature/*`, `fix/*`, `chore/*`

## Issue Tracking

Uses **beads** (`bd`). Run `bd ready` for available work, `bd prime` for full workflow context.

## Status

Phases 1-2 complete (all 5 providers working, basic TUI). Next: integration/E2E tests, then search, export, hardening.
