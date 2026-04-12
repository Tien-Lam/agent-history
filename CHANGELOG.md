# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.1] - 2026-04-12

### Fixed

- **Security**: Escape HTML language attribute in code block export to prevent XSS injection
- **Security**: Shell-escape session IDs in resume commands to prevent command injection via clipboard
- **Security**: Sanitize export filenames to prevent path traversal via crafted session IDs
- **Safety**: Install panic hook to restore terminal on crash (raw mode + alternate screen)
- **Safety**: Use UTF-8-safe string slicing for session ID truncation and Codex UUID extraction
- Help toggle now preserves previous mode (was always returning to Browse from ViewSession)
- Clear stale search results when session list is reloaded
- Warn on malformed `config.toml` instead of silently falling back to defaults
- Log warning on Windows uninstall cleanup failure instead of ignoring

### Changed

- Migrated from deprecated `serde_yaml` 0.9 to `serde_yml` 0.0.12
- Bumped `lru` to 0.16.3 to fix Stacked Borrows soundness issue

### Added

- Tests for HTML attribute injection, Unicode/CJK/RTL export, IO error paths, help mode preservation, and export-while-filtered workflow

## [0.2.0] - 2026-04-12

### Added

- Install script (`install.sh`) for one-line installation on Linux and macOS
- `aghist update` subcommand for self-updating from GitHub releases
- `aghist uninstall` subcommand to remove binary, search index, and config
- `cargo binstall` support via package metadata
- Architecture documentation (`docs/ARCHITECTURE.md`)
- LICENSE file (MIT)
- CHANGELOG.md

## [0.1.3] - 2026-04-11

### Changed

- Migrated tests to shared helpers, added search and export workflow tests
- Strengthened tests to assert real behavior instead of just "didn't crash"
- Removed mocked dispatch calls in favor of exercising the real event loop

## [0.1.2] - 2026-04-10

### Added

- Resume command with clipboard copy for all providers (`y` keybinding)
- Cross-platform release workflow (Linux x86_64, Windows x86_64, macOS aarch64)
- Demo tape for recording GIF with VHS

### Fixed

- Windows key handling and UI rendering
- Filter editing, project name display, config loading, and assorted UX issues

## [0.1.0] - 2026-04-10

### Added

- TUI with session browsing, message viewing, and vim-style keybindings
- Five provider parsers: Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode
- Full-text search powered by Tantivy with incremental indexing
- Filter panel (provider, project, date range)
- Export to Markdown, JSON, and HTML
- Configuration via TOML (`~/.config/aghist/config.toml`)
- LRU message cache
- GitHub Actions CI (clippy, tests, build)
- Snapshot tests with insta

[0.2.0]: https://github.com/Tien-Lam/agent-history/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/Tien-Lam/agent-history/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Tien-Lam/agent-history/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/Tien-Lam/agent-history/releases/tag/v0.1.0
