# aghist

Cross-platform TUI for viewing and searching AI agent conversation history.

[![CI](https://github.com/Tien-Lam/agent-history/actions/workflows/ci.yml/badge.svg)](https://github.com/Tien-Lam/agent-history/actions/workflows/ci.yml)

## Supported Providers

- **Claude Code** — `~/.claude/projects/` conversations
- **Copilot CLI** — GitHub Copilot CLI history
- **Gemini CLI** — Google Gemini CLI history
- **Codex CLI** — OpenAI Codex CLI history
- **OpenCode** — OpenCode conversations

Providers are auto-detected based on platform-specific default paths.

## Features

- Browse sessions across all providers in a unified TUI
- Full-text search powered by Tantivy
- Filter by provider, date range, and project
- Export sessions to Markdown, JSON, or HTML
- Configurable via `~/.config/aghist/config.toml`
- No async — fast startup, low resource usage

## Installation

### From source

```sh
cargo install --path .
```

### Build from Git

```sh
git clone https://github.com/Tien-Lam/agent-history.git
cd agent-history
cargo build --release
# Binary at target/release/aghist (.exe on Windows)
```

## Usage

```sh
aghist                    # Launch TUI
aghist --list             # List sessions without TUI
aghist --reindex          # Rebuild search index
aghist export -f md -s <session-id>           # Export to Markdown
aghist export -f json -s <session-id> -o out.json  # Export to JSON file
```

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
```

## License

MIT
