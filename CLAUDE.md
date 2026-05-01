# aghist

Cross-platform TUI for viewing and searching AI agent conversation history (Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode). See [`AGENTS.md`](AGENTS.md) for agent-specific workflow rules.

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

## Issue Tracking

Uses **beads** (`bd`). Run `bd ready` for available work, `bd prime` for full workflow context.


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
