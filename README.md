# aghist

Cross-platform TUI for viewing and searching AI agent conversation history.

[![CI](https://github.com/Tien-Lam/agent-history/actions/workflows/ci.yml/badge.svg)](https://github.com/Tien-Lam/agent-history/actions/workflows/ci.yml)

<!-- To record: install VHS (https://github.com/charmbracelet/vhs) and run `vhs demo.tape` -->
![demo](demo.gif)

## Supported Providers

- **Claude Code** — `~/.claude/projects/` conversations
- **Copilot CLI** — GitHub Copilot CLI history
- **Gemini CLI** — Google Gemini CLI history
- **Codex CLI** — OpenAI Codex CLI history
- **OpenCode** — OpenCode conversations
- **Cursor** — Cursor composer/chat history
- **Aider** — per-repository `.aider.chat.history.md` transcripts
- **Zed AI** — Zed assistant conversations
- **Cline** — Cline task histories from VS Code-compatible editors
- **Continue.dev** — Continue session JSONL history

Providers are auto-detected based on platform-specific default paths.

## Features

- Browse sessions across all providers in a unified TUI
- Full-text search (Tantivy BM25, optional FastEmbed semantic + RRF hybrid)
- Filter by provider, project, date range, role, presence of tool calls
- Stable citation refs: `<provider>/<session-id>#<turn>` round-trip across reindex
- Export sessions to Markdown, JSON, or HTML; slice by `--turn-range`
- Built-in `aghist mcp` stdio MCP server — agents can query their own history without parsing CLI
- `aghist sources add/list/remove/pull` — aggregate sessions across hosts via rsync, federated search across local + remote caches
- Heuristic `decisions` / `todos` / `threads` extractors for cross-session retrospectives
- Stable error envelope, semantic exit codes, JSON-Schema introspection (`aghist schema`)
- No async runtime — fast startup, low resource usage

## Installation

### Shell (Linux / macOS)

```sh
curl -sSfL https://raw.githubusercontent.com/Tien-Lam/agent-history/main/install.sh | bash
```

Installs to `~/.local/bin` by default. Override with
`| bash -s -- --to /path/to/bin`; the directory must be writable by the
installing user.

### cargo-binstall

```sh
cargo binstall aghist
```

### From source

```sh
cargo install --git https://github.com/Tien-Lam/agent-history.git
```

### Manual download

Download the latest release for your platform from [GitHub Releases](https://github.com/Tien-Lam/agent-history/releases).

| Platform | Archive |
|----------|---------|
| Linux x86_64 | `aghist-v*-x86_64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `aghist-v*-x86_64-pc-windows-msvc.zip` |
| macOS Apple Silicon | `aghist-v*-aarch64-apple-darwin.tar.gz` |

### Updating

```sh
aghist update
```

`aghist update` is for GitHub release binaries installed by the shell script,
which writes an adjacent `aghist.install` marker so aghist can tell it owns the
binary. If you installed with `cargo-binstall`, run
`cargo binstall aghist --force`; if you installed from source, run
`cargo install --git https://github.com/Tien-Lam/agent-history.git --force`.
Package-manager and manually copied binaries should be updated with the tool or
process that installed them.

### Uninstalling

```sh
aghist uninstall
```

Removes a self-managed release binary installed by the shell script, plus the
search index and configuration. For Cargo, package-manager, or manually copied
installs, uninstall the binary with that installer/process and remove the data
directories manually if desired.

## Usage

### TUI

```sh
aghist                    # Launch TUI
aghist --list             # List sessions to stdout (table format on TTY, JSON on pipe)
aghist --reindex          # Rebuild search index from scratch
```

### Agent-friendly CLI

aghist's command surface is designed to be scriptable. Every subcommand:

- emits stable JSON on a pipe (or with `--json`) and a human-readable table on a TTY
- uses semantic exit codes — `0` success, `1` runtime error, `2` usage error, `3` success-but-empty
- on error, writes a single-line JSON envelope `{"error":{"kind":"…","message":"…","hint":"…"}}` to stderr
- has a discoverable JSON-Schema for its params and response (`aghist schema <subcmd>`)

```sh
# Search
aghist search "reflection"                                  # BM25 lexical search
aghist search "reflection" --hybrid-weight 0.5              # RRF blend with FastEmbed (opt-in)
aghist search --query-file /tmp/q.txt                       # query from file
echo "..." | aghist search --stdin                          # query from stdin
aghist search --params '{"query":"x","limit":5}'            # whole request as one JSON
aghist search "x" --debug-search                            # show BM25 explanation tree per hit
aghist search "x" --watch                                   # stream NDJSON as new sessions land

# Filters (work on --list, search, decisions, todos, threads)
aghist --provider claude-code --since 2025-01-01T00:00:00Z --list
aghist search "x" --role assistant --has-tool-call --project myrepo

# Pagination
aghist --list --limit 50 --json | jq '.meta.next_cursor'    # cursor in response
aghist --list --limit 50 --cursor "$CURSOR" --json          # continue

# Resolve a citation ref
aghist show claude-code/abc-123#7                           # one message
aghist show claude-code/abc-123#7 --include-context 3       # ±3 turns of context

# Export
aghist export -f md -s <session-id>                         # full session
aghist export -f md -s <id> --turn-range 12:25              # 1-based inclusive slice
aghist export -f json -s <id> -o out.json

# Index management
aghist index                                                # idempotent, delta-aware
aghist index --force                                        # full rebuild
aghist index --accept-download                              # +FastEmbed semantic embeddings (~90 MB)
aghist health --json                                        # machine-readable doctor
aghist sources                                              # provider paths + sizes + last-indexed-at

# Cross-session research
aghist decisions --limit 20 --json                          # heuristic decision extraction
aghist todos --json                                         # unresolved TODOs/follow-ups
aghist threads --json                                       # cluster sessions by project + time

# Cross-machine sources
aghist sources add work --host me@laptop --path ~/.claude   # register a remote
aghist sources pull work                                    # rsync remote → local cache
aghist sources pull --all                                   # pull everything
aghist search "x"                                           # federates across local + remote caches

# JSON-Schema introspection (for agent self-discovery)
aghist schema --list                                        # available subcommands
aghist schema search                                        # full draft-2020-12 schema for `search`

# MCP stdio server (one process per client)
aghist mcp                                                  # JSON-RPC 2.0 over stdio
                                                            # tools: search_sessions, list_sessions,
                                                            # get_session, get_message, reindex, health
                                                            # resources: aghist://session/<provider>/<id>
```

For the full schema of every subcommand, run `aghist schema <subcmd>`.

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` / `↑` / `↓` | Navigate sessions / scroll |
| `Enter` | Open session messages |
| `/` | Start search |
| `f` | Toggle filter panel |
| `e` | Export session (in message view) |
| `t` | Toggle tool call visibility |
| `g` / `G` | Jump to top / bottom |
| `Esc` | Back / close overlay |
| `?` | Help |
| `q` | Quit |

## Configuration

Create `~/.config/aghist/config.toml` (or `%APPDATA%\aghist\config.toml` on Windows):

```toml
cache_size = 20
show_tool_calls = false
max_messages_per_session = 5000

[providers]
enabled = ["claude-code", "copilot-cli", "gemini-cli", "codex-cli", "opencode"]
# Optional MCP scoping: only expose this subset to `aghist mcp` clients.
# Defaults to `enabled` when omitted.
# mcp_exposed = ["claude-code"]

# Cross-machine remote sources (managed via `aghist sources add/remove`).
# [[sources]]
# name = "laptop"
# host = "me@laptop"
# path = "~/.claude"
# transport = "ssh"  # or "rsync"
```

## License

MIT
