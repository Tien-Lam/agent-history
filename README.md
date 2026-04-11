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

Providers are auto-detected based on platform-specific default paths.

## Features

- Browse sessions across all providers in a unified TUI
- Full-text search powered by Tantivy
- Filter by provider, date range, and project
- Export sessions to Markdown, JSON, or HTML
- Configurable via `~/.config/aghist/config.toml`
- No async — fast startup, low resource usage

## Installation

### Shell (Linux / macOS)

```sh
curl -sSfL https://raw.githubusercontent.com/Tien-Lam/agent-history/main/install.sh | bash
```

Installs to `~/.local/bin` by default. Override with `| bash -s -- --to /usr/local/bin`.

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

### Uninstalling

```sh
aghist uninstall
```

Removes the binary, search index, and configuration.

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
